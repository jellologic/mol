// SPDX-License-Identifier: MIT OR Apache-2.0
//! Load-map construction and canonical dumps (P1.1.V1).
//!
//! [`LoadMap`] summarizes one thin image: slid segments, entry,
//! linked dylibs, codesign policy inputs, exports, and every fixup.
//! [`LoadMap::canonical`] renders a stable text form; golden files
//! hold oracle-verified dumps and the test asserts byte equality.
use crate::chained::{self, FixupKind, SegRange};
use crate::codesign;
use crate::header::{self, Header64};
use crate::load::{self, LoadCommand};
use crate::opcodes::{self, DEFAULT_EVENT_BUDGET};
use crate::trie;
use crate::{ErrorKind, ParseError, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSection {
    pub sectname: String,
    pub addr: u64,
    pub size: u64,
    pub offset: u32,
    pub flags: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSegment {
    pub segname: String,
    pub vmaddr: u64,
    pub vmsize: u64,
    pub fileoff: u64,
    pub filesize: u64,
    pub maxprot: u32,
    pub initprot: u32,
    pub flags: u32,
    pub sections: Vec<MapSection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapEntry {
    /// Preferred (unslid) entry vmaddr.
    pub vmaddr: u64,
    pub stacksize: u64,
    pub source: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapDylib {
    pub cmd: u32,
    pub name: String,
    pub current: (u16, u8, u8),
    pub compat: (u16, u8, u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapCodesign {
    pub ident: String,
    pub flags: u32,
    pub team_id: Option<String>,
    pub gating: Vec<(String, bool)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapExport {
    pub name: String,
    pub flags: u64,
    pub address: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthDesc {
    pub diversity: u16,
    pub addr_div: bool,
    pub key: u8,
}

/// Structured fixup operation. The loader/dyld engine matches on
/// this; [`MapFixup::detail`] renders the stable golden text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixupOp {
    ChainRebase {
        target: u64,
        high8: u8,
        is_offset: bool,
        /// Preferred (unslid) target, resolved at build time.
        resolved: u64,
        auth: Option<AuthDesc>,
    },
    ChainBind {
        ordinal: u32,
        addend: i64,
        lib: i64,
        symbol: String,
        weak: bool,
        auth: Option<AuthDesc>,
    },
    InfoRebase {
        kind: u8,
    },
    InfoBind {
        kind: u8,
        stream: &'static str,
        lib: i64,
        symbol: String,
        addend: i64,
        weak: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapFixup {
    /// Preferred (unslid) vmaddr of the fixup location.
    pub vmaddr: u64,
    pub seg_index: usize,
    pub seg_offset: u64,
    pub op: FixupOp,
}

impl MapFixup {
    /// Stable one-line rendering (golden format; the oracle test
    /// parses `ord=`/`lib=` tokens from bind lines).
    pub fn detail(&self) -> String {
        match &self.op {
            FixupOp::ChainRebase { resolved, auth, .. } => match auth {
                Some(a) => format!(
                    "chain rebase -> {resolved:#x} auth(div={:#x},div={:?},key={})",
                    a.diversity, a.addr_div, a.key
                ),
                None => format!("chain rebase -> {resolved:#x}"),
            },
            FixupOp::ChainBind {
                ordinal,
                addend,
                lib,
                symbol,
                weak,
                auth,
            } => {
                let w = if *weak { " weak" } else { "" };
                match auth {
                    Some(a) => format!(
                        "chain bind ord={ordinal} lib={lib} {symbol}+{addend}{w} auth(div={:#x},key={})",
                        a.diversity, a.key
                    ),
                    None => format!("chain bind ord={ordinal} lib={lib} {symbol}+{addend}{w}"),
                }
            }
            FixupOp::InfoRebase { kind } => format!("dyldinfo rebase type={kind}"),
            FixupOp::InfoBind {
                stream,
                lib,
                symbol,
                addend,
                weak,
                ..
            } => {
                let w = if *weak { " weak" } else { "" };
                format!("dyldinfo {stream} lib={lib} {symbol}+{addend}{w}")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadMap {
    pub cputype: u32,
    pub cpusubtype: u32,
    pub filetype: u32,
    pub flags: u32,
    pub slide: u64,
    pub segments: Vec<MapSegment>,
    pub entry: Option<MapEntry>,
    /// `LC_ROUTINES_64` (init_address, init_module), file order.
    pub routines: Vec<(u64, u64)>,
    pub dylinker: Option<String>,
    pub dylibs: Vec<MapDylib>,
    pub uuid: Option<[u8; 16]>,
    pub build: Option<(u32, u32, u32)>,
    pub codesign: Option<MapCodesign>,
    pub exports: Vec<MapExport>,
    pub fixups: Vec<MapFixup>,
    pub unknown_cmds: Vec<(u32, u32)>,
}

fn payload(file: &[u8], off: u32, size: u32) -> Result<&[u8]> {
    let (off, size) = (off as usize, size as usize);
    let end = off.checked_add(size).ok_or(ParseError {
        offset: off,
        kind: ErrorKind::OffsetOverflow,
    })?;
    if end > file.len() {
        return Err(ParseError {
            offset: off,
            kind: ErrorKind::PayloadOutOfRange {
                off,
                size,
                len: file.len(),
            },
        });
    }
    Ok(&file[off..end])
}

fn seg_ranges(segments: &[MapSegment]) -> Vec<SegRange> {
    segments
        .iter()
        .map(|s| SegRange {
            vmaddr: s.vmaddr,
            vmsize: s.vmsize,
            fileoff: s.fileoff,
            filesize: s.filesize,
        })
        .collect()
}

fn seg_offset_to_vmaddr(segments: &[MapSegment], seg: usize, off: u64) -> Option<u64> {
    segments.get(seg).and_then(|s| {
        if off < s.vmsize {
            s.vmaddr.checked_add(off)
        } else {
            None
        }
    })
}

/// Build the load map for one thin image at `slide`.
pub fn build(
    header: &Header64,
    cmds: &[LoadCommand<'_>],
    file: &[u8],
    slide: u64,
) -> Result<LoadMap> {
    let mut map = LoadMap {
        cputype: header.cputype,
        cpusubtype: header.cpusubtype,
        filetype: header.filetype,
        flags: header.flags,
        slide,
        segments: Vec::new(),
        entry: None,
        routines: Vec::new(),
        dylinker: None,
        dylibs: Vec::new(),
        uuid: None,
        build: None,
        codesign: None,
        exports: Vec::new(),
        fixups: Vec::new(),
        unknown_cmds: Vec::new(),
    };
    // Pass 1: segments, entry, links, misc.
    let mut thread_entry: Option<(u32, Vec<u8>)> = None;
    let mut main_entry: Option<(u64, u64)> = None;
    for cmd in cmds {
        match cmd {
            LoadCommand::Segment(s) => map.segments.push(MapSegment {
                segname: s.segname.to_string(),
                vmaddr: s.vmaddr,
                vmsize: s.vmsize,
                fileoff: s.fileoff,
                filesize: s.filesize,
                maxprot: s.maxprot,
                initprot: s.initprot,
                flags: s.flags,
                sections: s
                    .sections
                    .iter()
                    .map(|x| MapSection {
                        sectname: x.sectname.to_string(),
                        addr: x.addr,
                        size: x.size,
                        offset: x.offset,
                        flags: x.flags,
                    })
                    .collect(),
            }),
            LoadCommand::EntryPoint(e) => {
                main_entry = Some((e.entryoff, e.stacksize));
            }
            LoadCommand::Thread(t) => {
                thread_entry = Some((t.flavor, t.state.to_vec()));
            }
            LoadCommand::Routines(r) => {
                map.routines.push((r.init_address, r.init_module));
            }
            LoadCommand::Dylinker(d) => map.dylinker = Some(d.name.to_string()),
            LoadCommand::Dylib(d) => map.dylibs.push(MapDylib {
                cmd: d.cmd,
                name: d.name.to_string(),
                current: load::decode_version(d.current_version),
                compat: load::decode_version(d.compatibility_version),
            }),
            LoadCommand::Uuid(u) => map.uuid = Some(u.uuid),
            LoadCommand::BuildVersion(b) => {
                map.build = Some((b.platform, b.minos, b.sdk));
            }
            LoadCommand::Unknown { cmd, cmdsize, .. } => {
                map.unknown_cmds.push((*cmd, *cmdsize));
            }
            _ => {}
        }
    }
    // Entry: LC_MAIN wins (modern binaries); else thread-state PC.
    if let Some((entryoff, stacksize)) = main_entry {
        let vmaddr = file_offset_to_vmaddr(&map.segments, entryoff).ok_or(ParseError {
            offset: 0,
            kind: ErrorKind::EntryOutsideSegments { entryoff },
        })?;
        map.entry = Some(MapEntry {
            vmaddr,
            stacksize,
            source: "LC_MAIN",
        });
    } else if let Some((flavor, state)) = thread_entry {
        let pc = thread_pc(flavor, &state)?;
        map.entry = Some(MapEntry {
            vmaddr: pc,
            stacksize: 0,
            source: "LC_UNIXTHREAD",
        });
    }
    // Pass 2: payloads needing the segment table. Destructure so
    // the segment table and the output vecs borrow disjointly.
    let LoadMap {
        segments,
        codesign,
        exports,
        fixups,
        ..
    } = &mut map;
    for cmd in cmds {
        match cmd {
            LoadCommand::LinkeditData(d) => match d.cmd {
                load::LC_CODE_SIGNATURE => {
                    let bytes = payload(file, d.dataoff, d.datasize)?;
                    let sig = codesign::parse(bytes)?;
                    if let Some(dir) = sig.code_directory() {
                        *codesign = Some(MapCodesign {
                            ident: dir.ident.to_string(),
                            flags: dir.flags,
                            team_id: dir.team_id.map(str::to_string),
                            gating: sig
                                .gating_entitlements()
                                .into_iter()
                                .map(|(k, v)| (k.to_string(), v))
                                .collect(),
                        });
                    }
                }
                load::LC_DYLD_EXPORTS_TRIE => {
                    let bytes = payload(file, d.dataoff, d.datasize)?;
                    for e in trie::walk(bytes)? {
                        exports.push(MapExport {
                            name: e.name,
                            flags: e.flags,
                            address: e.address,
                        });
                    }
                }
                load::LC_DYLD_CHAINED_FIXUPS => {
                    let bytes = payload(file, d.dataoff, d.datasize)?;
                    collect_chained(bytes, file, segments, fixups)?;
                }
                _ => {}
            },
            LoadCommand::DyldInfo(d) => {
                collect_dyld_info(d, file, segments, fixups, exports)?;
            }
            _ => {}
        }
    }
    map.exports.sort_by(|a, b| a.name.cmp(&b.name));
    map.fixups.sort_by_key(|f| (f.vmaddr, f.detail()));
    Ok(map)
}

fn file_offset_to_vmaddr(segments: &[MapSegment], off: u64) -> Option<u64> {
    for s in segments {
        if off >= s.fileoff && off < s.fileoff + s.filesize {
            return s.vmaddr.checked_add(off - s.fileoff);
        }
    }
    None
}

/// Program counter out of an `LC_UNIXTHREAD` state blob.
fn thread_pc(flavor: u32, state: &[u8]) -> Result<u64> {
    // x86_THREAD_STATE64: rip is word 16; ARM_THREAD_STATE64: pc word 32.
    let word: usize = match flavor {
        load::X86_THREAD_STATE64 => 16,
        load::ARM_THREAD_STATE64 => 32,
        f => {
            return Err(ParseError {
                offset: 0,
                kind: ErrorKind::UnknownThreadFlavor(f),
            })
        }
    };
    let at = word * 8;
    if state.len() < at + 8 {
        return Err(ParseError {
            offset: 0,
            kind: ErrorKind::Truncated {
                need: at + 8,
                len: state.len(),
            },
        });
    }
    let mut w = [0u8; 8];
    w.copy_from_slice(&state[at..at + 8]);
    Ok(u64::from_le_bytes(w))
}

fn collect_chained(
    bytes: &[u8],
    file: &[u8],
    segments: &[MapSegment],
    out: &mut Vec<MapFixup>,
) -> Result<()> {
    let cf = chained::ChainedFixups::parse(bytes)?;
    let imports = cf.imports()?;
    let segs = seg_ranges(segments);
    let base = segments.first().map(|s| s.vmaddr).unwrap_or(0);
    let mut push = |fx: chained::Fixup| -> Result<()> {
        let op = match fx.kind {
            FixupKind::Rebase {
                target,
                high8,
                is_offset,
                auth,
            } => {
                let resolved = chained::resolve_rebase_target(target, high8, is_offset, base);
                FixupOp::ChainRebase {
                    target,
                    high8,
                    is_offset,
                    resolved,
                    auth: auth.map(|a| AuthDesc {
                        diversity: a.diversity,
                        addr_div: a.addr_div,
                        key: a.key,
                    }),
                }
            }
            FixupKind::Bind {
                ordinal,
                addend,
                auth,
            } => {
                let imp = imports.get(ordinal as usize);
                let symbol = imp.map(|i| i.name).unwrap_or("<bad-ordinal>").to_string();
                let lib = imp.map(|i| i.lib_ordinal).unwrap_or(-99);
                // IMPORT_ADDEND(64) formats carry the addend in the
                // imports table; the chain entry contributes its own
                // (usually zero). dyld sums both.
                let total = addend.wrapping_add(imp.map(|i| i.addend).unwrap_or(0));
                FixupOp::ChainBind {
                    ordinal,
                    addend: total,
                    lib,
                    symbol,
                    weak: imp.map(|i| i.weak).unwrap_or(false),
                    auth: auth.map(|a| AuthDesc {
                        diversity: a.diversity,
                        addr_div: a.addr_div,
                        key: a.key,
                    }),
                }
            }
        };
        out.push(MapFixup {
            vmaddr: fx.vmaddr,
            seg_index: fx.seg_index,
            seg_offset: fx.seg_offset,
            op,
        });
        Ok(())
    };
    cf.each_fixup(file, &segs, DEFAULT_EVENT_BUDGET, &mut push)?;
    Ok(())
}

fn collect_dyld_info(
    d: &load::DyldInfo,
    file: &[u8],
    segments: &[MapSegment],
    out: &mut Vec<MapFixup>,
    exports: &mut Vec<MapExport>,
) -> Result<()> {
    if d.rebase_size > 0 {
        let stream = payload(file, d.rebase_off, d.rebase_size)?;
        for r in opcodes::walk_rebase(stream, DEFAULT_EVENT_BUDGET)? {
            let vmaddr = seg_offset_to_vmaddr(segments, r.seg_index as usize, r.seg_offset).ok_or(
                ParseError {
                    offset: 0,
                    kind: ErrorKind::FixupOutsideSegment {
                        vmaddr: r.seg_offset,
                    },
                },
            )?;
            out.push(MapFixup {
                vmaddr,
                seg_index: r.seg_index as usize,
                seg_offset: r.seg_offset,
                op: FixupOp::InfoRebase { kind: r.kind },
            });
        }
    }
    for (off, size, tag) in [
        (d.bind_off, d.bind_size, "bind"),
        (d.weak_bind_off, d.weak_bind_size, "weak_bind"),
        (d.lazy_bind_off, d.lazy_bind_size, "lazy_bind"),
    ] {
        if size == 0 {
            continue;
        }
        let stream = payload(file, off, size)?;
        for b in opcodes::walk_bind(stream, DEFAULT_EVENT_BUDGET)? {
            let vmaddr = seg_offset_to_vmaddr(segments, b.seg_index as usize, b.seg_offset).ok_or(
                ParseError {
                    offset: 0,
                    kind: ErrorKind::FixupOutsideSegment {
                        vmaddr: b.seg_offset,
                    },
                },
            )?;
            out.push(MapFixup {
                vmaddr,
                seg_index: b.seg_index as usize,
                seg_offset: b.seg_offset,
                op: FixupOp::InfoBind {
                    kind: b.kind,
                    stream: tag,
                    lib: b.lib_ordinal,
                    symbol: b.symbol.to_string(),
                    addend: b.addend,
                    weak: b.weak_import,
                },
            });
        }
    }
    if d.export_size > 0 {
        // Same trie encoding as LC_DYLD_EXPORTS_TRIE.
        let bytes = payload(file, d.export_off, d.export_size)?;
        for e in trie::walk(bytes)? {
            exports.push(MapExport {
                name: e.name,
                flags: e.flags,
                address: e.address,
            });
        }
    }
    Ok(())
}

impl LoadMap {
    /// Stable text rendering for golden comparison. All addresses are
    /// preferred (unslid) unless the line says `slid`.
    pub fn canonical(&self) -> String {
        let mut s = String::new();
        s.push_str("# mol load-map v1\n");
        s.push_str(&format!(
            "arch: {} subtype={:#x}\n",
            header::cputype_name(self.cputype),
            self.cpusubtype
        ));
        s.push_str(&format!(
            "header: filetype={} flags={:#x} slide={:#x}\n",
            header::filetype_name(self.filetype),
            self.flags,
            self.slide
        ));
        for seg in &self.segments {
            s.push_str(&format!(
                "segment {} vmaddr={:#x} slid={:#x} vmsize={:#x} fileoff={:#x} filesize={:#x} maxprot={:#x} initprot={:#x} flags={:#x}\n",
                seg.segname,
                seg.vmaddr,
                seg.vmaddr.wrapping_add(self.slide),
                seg.vmsize,
                seg.fileoff,
                seg.filesize,
                seg.maxprot,
                seg.initprot,
                seg.flags
            ));
            for sc in &seg.sections {
                s.push_str(&format!(
                    "  section {} addr={:#x} slid={:#x} size={:#x} offset={:#x} flags={:#x}\n",
                    sc.sectname,
                    sc.addr,
                    sc.addr.wrapping_add(self.slide),
                    sc.size,
                    sc.offset,
                    sc.flags
                ));
            }
        }
        match &self.entry {
            Some(e) => s.push_str(&format!(
                "entry: {:#x} slid={:#x} stacksize={:#x} via={}\n",
                e.vmaddr,
                e.vmaddr.wrapping_add(self.slide),
                e.stacksize,
                e.source
            )),
            None => s.push_str("entry: none\n"),
        }
        for (addr, module) in &self.routines {
            s.push_str(&format!(
                "init: {:#x} slid={:#x} module={:#x}\n",
                addr,
                addr.wrapping_add(self.slide),
                module
            ));
        }
        s.push_str(&format!(
            "dylinker: {}\n",
            self.dylinker.as_deref().unwrap_or("none")
        ));
        for d in &self.dylibs {
            s.push_str(&format!(
                "dylib {} {} current={}.{}.{} compat={}.{}.{}\n",
                load::cmd_name(d.cmd),
                d.name,
                d.current.0,
                d.current.1,
                d.current.2,
                d.compat.0,
                d.compat.1,
                d.compat.2
            ));
        }
        match &self.uuid {
            Some(u) => s.push_str(&format!("uuid: {}\n", uuid_hex(u))),
            None => s.push_str("uuid: none\n"),
        }
        match self.build {
            Some((p, minos, sdk)) => {
                let (a, b, c) = load::decode_version(minos);
                let (x, y, z) = load::decode_version(sdk);
                s.push_str(&format!(
                    "build: platform={} minos={a}.{b}.{c} sdk={x}.{y}.{z}\n",
                    load::platform_name(p)
                ));
            }
            None => s.push_str("build: none\n"),
        }
        match &self.codesign {
            Some(c) => {
                s.push_str(&format!(
                    "codesign: ident={} flags={:#x} team={}\n",
                    c.ident,
                    c.flags,
                    c.team_id.as_deref().unwrap_or("none")
                ));
                for (k, v) in &c.gating {
                    s.push_str(&format!("  gating: {k}={v}\n"));
                }
            }
            None => s.push_str("codesign: none\n"),
        }
        s.push_str(&format!("exports: {}\n", self.exports.len()));
        for e in &self.exports {
            s.push_str(&format!(
                "  export {:#x} flags={:#x} {}\n",
                e.address, e.flags, e.name
            ));
        }
        s.push_str(&format!("fixups: {}\n", self.fixups.len()));
        for f in &self.fixups {
            s.push_str(&format!(
                "  fixup {:#x} slid={:#x} seg={} off={:#x} {}\n",
                f.vmaddr,
                f.vmaddr.wrapping_add(self.slide),
                f.seg_index,
                f.seg_offset,
                f.detail()
            ));
        }
        for (cmd, size) in &self.unknown_cmds {
            s.push_str(&format!("unknown_cmd: {cmd:#x} size={size}\n"));
        }
        s
    }
}

fn uuid_hex(u: &[u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(36);
    for (i, b) in u.iter().enumerate() {
        if i == 4 || i == 6 || i == 8 || i == 10 {
            s.push('-');
        }
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::{CPU_TYPE_X86_64, MH_EXECUTE};
    use crate::load::{EntryPoint, Thread, X86_THREAD_STATE64};

    fn header() -> Header64 {
        Header64 {
            cputype: CPU_TYPE_X86_64,
            cpusubtype: 3,
            filetype: MH_EXECUTE,
            flags: 0x85, // no MH_PIE: classic non-PIE layout
            ncmds: 0,
            sizeofcmds: 0,
        }
    }

    fn text_segment() -> crate::load::Segment<'static> {
        crate::load::Segment {
            segname: "__TEXT",
            vmaddr: 0x1000_0000,
            vmsize: 0x1000,
            fileoff: 0,
            filesize: 0x1000,
            maxprot: 5,
            initprot: 5,
            flags: 0,
            sections: Vec::new(),
        }
    }

    #[test]
    fn unixthread_entry_resolves_pc_without_lc_main() {
        let mut state = vec![0u8; 21 * 8];
        state[16 * 8..17 * 8].copy_from_slice(&0x1000_0f50u64.to_le_bytes());
        let cmds = vec![
            LoadCommand::Segment(text_segment()),
            LoadCommand::Thread(Thread {
                flavor: X86_THREAD_STATE64,
                count: 21,
                state: Box::leak(state.into_boxed_slice()),
            }),
        ];
        let map = build(&header(), &cmds, &[0u8; 0x1000], 0).unwrap();
        let entry = map.entry.unwrap();
        assert_eq!(entry.vmaddr, 0x1000_0f50);
        assert_eq!(entry.source, "LC_UNIXTHREAD");
    }

    #[test]
    fn lc_main_wins_over_thread_and_validates_range() {
        let mut state = vec![0u8; 21 * 8];
        state[16 * 8..17 * 8].copy_from_slice(&0xdead_beefu64.to_le_bytes());
        let cmds = vec![
            LoadCommand::Segment(text_segment()),
            LoadCommand::Thread(Thread {
                flavor: X86_THREAD_STATE64,
                count: 21,
                state: Box::leak(state.into_boxed_slice()),
            }),
            LoadCommand::EntryPoint(EntryPoint {
                entryoff: 0xf50,
                stacksize: 0,
            }),
        ];
        let map = build(&header(), &cmds, &[0u8; 0x1000], 0).unwrap();
        let entry = map.entry.unwrap();
        assert_eq!(entry.vmaddr, 0x1000_0f50);
        assert_eq!(entry.source, "LC_MAIN");

        // Entry outside every segment is a hard error, not a guess.
        let bad = vec![
            LoadCommand::Segment(text_segment()),
            LoadCommand::EntryPoint(EntryPoint {
                entryoff: 0x5000,
                stacksize: 0,
            }),
        ];
        assert!(matches!(
            build(&header(), &bad, &[0u8; 0x1000], 0).unwrap_err().kind,
            ErrorKind::EntryOutsideSegments { .. }
        ));
    }
}
