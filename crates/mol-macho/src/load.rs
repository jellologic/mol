// SPDX-License-Identifier: MIT OR Apache-2.0
//! Load-command iteration and typed decoding.
//!
//! [`Commands`] walks the command table with overflow-checked
//! arithmetic; [`decode`] turns each raw command into a typed
//! [`LoadCommand`]. Unknown commands are preserved verbatim so the
//! loader can warn without failing (P1.1.1).
use crate::cursor::Cursor;
use crate::{ErrorKind, ParseError, Result};

// --- command codes ---
pub const LC_REQ_DYLD: u32 = 0x8000_0000;
pub const LC_SEGMENT: u32 = 0x1;
pub const LC_UUID: u32 = 0x1b;
pub const LC_SYMTAB: u32 = 0x2;
pub const LC_DYSYMTAB: u32 = 0xb;
pub const LC_LOAD_DYLIB: u32 = 0xc;
pub const LC_ID_DYLIB: u32 = 0xd;
pub const LC_LOAD_DYLINKER: u32 = 0xe;
pub const LC_PREBOUND_DYLIB: u32 = 0x10;
pub const LC_ROUTINES_64: u32 = 0x1a;
pub const LC_SUB_FRAMEWORK: u32 = 0x12;
pub const LC_SUB_UMBRELLA: u32 = 0x13;
pub const LC_SUB_CLIENT: u32 = 0x14;
pub const LC_SUB_LIBRARY: u32 = 0x15;
pub const LC_SEGMENT_64: u32 = 0x19;
pub const LC_CODE_SIGNATURE: u32 = 0x1d;
pub const LC_SEGMENT_SPLIT_INFO: u32 = 0x1e;
pub const LC_LOAD_WEAK_DYLIB: u32 = 0x8000_0018;
pub const LC_REEXPORT_DYLIB: u32 = 0x8000_001f;
pub const LC_DYLD_INFO: u32 = 0x22;
pub const LC_DYLD_INFO_ONLY: u32 = 0x8000_0022;
pub const LC_LOAD_UPWARD_DYLIB: u32 = 0x8000_0023;
pub const LC_VERSION_MIN_MACOSX: u32 = 0x24;
pub const LC_VERSION_MIN_IPHONEOS: u32 = 0x25;
pub const LC_FUNCTION_STARTS: u32 = 0x26;
pub const LC_DYLD_ENVIRONMENT: u32 = 0x27;
pub const LC_MAIN: u32 = 0x8000_0028;
pub const LC_DATA_IN_CODE: u32 = 0x29;
pub const LC_SOURCE_VERSION: u32 = 0x2a;
pub const LC_DYLIB_CODE_SIGN_DRS: u32 = 0x2b;
pub const LC_ENCRYPTION_INFO_64: u32 = 0x2c;
pub const LC_LINKER_OPTION: u32 = 0x2d;
pub const LC_LINKER_OPTIMIZATION_HINT: u32 = 0x2e;
pub const LC_VERSION_MIN_TVOS: u32 = 0x2f;
pub const LC_VERSION_MIN_WATCHOS: u32 = 0x30;
pub const LC_BUILD_VERSION: u32 = 0x32;
pub const LC_DYLD_EXPORTS_TRIE: u32 = 0x8000_0033;
pub const LC_DYLD_CHAINED_FIXUPS: u32 = 0x8000_0034;
pub const LC_FILESET_ENTRY: u32 = 0x8000_0035;
pub const LC_UNIXTHREAD: u32 = 0x5;
pub const LC_LOADFVMLIB: u32 = 0x6;
pub const LC_IDFVMLIB: u32 = 0x7;
pub const LC_IDENT: u32 = 0x8;
pub const LC_FVMFILE: u32 = 0x9;
pub const LC_PREPAGE: u32 = 0xa;
pub const LC_RPATH: u32 = 0x8000_001c;
pub const LC_TWOLEVEL_HINTS: u32 = 0x16;
pub const LC_PREBIND_CKSUM: u32 = 0x17;
pub const LC_ID_DYLINKER: u32 = 0xf;
pub const LC_NOTE: u32 = 0x31;
pub const LC_ATOM_INFO: u32 = 0x36;
pub const LC_FUNCTION_VARIANTS: u32 = 0x37;
pub const LC_FUNCTION_VARIANT_FIXUPS: u32 = 0x38;
pub const LC_TARGET_TRIPLE: u32 = 0x39;
pub const LC_LAZY_LOAD_DYLIB_INFO: u32 = 0x3a;

// --- vm protections ---
pub const VM_PROT_READ: u32 = 0x1;
pub const VM_PROT_WRITE: u32 = 0x2;
pub const VM_PROT_EXECUTE: u32 = 0x4;

// --- thread flavors (whole `LC_UNIXTHREAD` state is kept raw) ---
pub const X86_THREAD_STATE64: u32 = 4;
pub const ARM_THREAD_STATE64: u32 = 6;

// --- platforms (LC_BUILD_VERSION) ---
pub const PLATFORM_MACOS: u32 = 1;
pub const PLATFORM_IOS: u32 = 2;
pub const PLATFORM_TVOS: u32 = 3;
pub const PLATFORM_WATCHOS: u32 = 4;
pub const PLATFORM_MACCATALYST: u32 = 6;
pub const PLATFORM_IOSSIMULATOR: u32 = 7;

/// A raw command: code, size, and the full command bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawCommand<'a> {
    pub cmd: u32,
    pub cmdsize: u32,
    pub offset: usize,
    pub bytes: &'a [u8],
}

/// Iterator over the load-command table of one thin image.
#[derive(Debug, Clone)]
pub struct Commands<'a> {
    cur: Cursor<'a>,
    off: usize,
    remaining: u32,
    table_end: usize,
}

impl<'a> Commands<'a> {
    /// `base` is the image slice start; `ncmds`/`sizeofcmds` come from
    /// the header. Fails if the table overruns the slice.
    pub fn new(bytes: &'a [u8], base: usize, ncmds: u32, sizeofcmds: u32) -> Result<Self> {
        let table = base
            .checked_add(crate::header::SIZEOF_MACH_HEADER_64)
            .ok_or(ParseError {
                offset: base,
                kind: ErrorKind::OffsetOverflow,
            })?;
        let table_end = table.checked_add(sizeofcmds as usize).ok_or(ParseError {
            offset: base,
            kind: ErrorKind::OffsetOverflow,
        })?;
        if table_end > bytes.len() {
            return Err(ParseError {
                offset: base,
                kind: ErrorKind::Truncated {
                    need: table_end,
                    len: bytes.len(),
                },
            });
        }
        Ok(Commands {
            cur: Cursor::new(bytes),
            off: table,
            remaining: ncmds,
            table_end,
        })
    }
}

impl<'a> Iterator for Commands<'a> {
    type Item = Result<RawCommand<'a>>;

    fn next(&mut self) -> Option<Result<RawCommand<'a>>> {
        let out = self.next_inner();
        // Fuse on error: a hostile ncmds must not buy billions of
        // failing iterations from callers that don't break (fuzz).
        if matches!(out, Some(Err(_))) {
            self.remaining = 0;
        }
        out
    }
}

impl<'a> Commands<'a> {
    fn next_inner(&mut self) -> Option<Result<RawCommand<'a>>> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let off = self.off;
        let cmd = match self.cur.u32le(off) {
            Ok(c) => c,
            Err(e) => return Some(Err(e)),
        };
        let cmdsize = match self.cur.u32le(off + 4) {
            Ok(s) => s,
            Err(e) => return Some(Err(e)),
        };
        if cmdsize < 8 {
            return Some(Err(ParseError {
                offset: off,
                kind: ErrorKind::CommandTooSmall { cmd, cmdsize },
            }));
        }
        let end = match off.checked_add(cmdsize as usize) {
            Some(e) => e,
            None => {
                return Some(Err(ParseError {
                    offset: off,
                    kind: ErrorKind::OffsetOverflow,
                }))
            }
        };
        if end > self.table_end {
            return Some(Err(ParseError {
                offset: off,
                kind: ErrorKind::CommandOverflow { cmd, cmdsize },
            }));
        }
        self.off = end;
        // Bounds were validated above (`end <= table_end <= len`).
        let bytes = match self.cur.bytes(off, cmdsize as usize) {
            Ok(b) => b,
            Err(e) => return Some(Err(e)),
        };
        Some(Ok(RawCommand {
            cmd,
            cmdsize,
            offset: off,
            bytes,
        }))
    }
}

// --- typed commands ---

/// `LC_SEGMENT_64` with its section list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment<'a> {
    pub segname: &'a str,
    pub vmaddr: u64,
    pub vmsize: u64,
    pub fileoff: u64,
    pub filesize: u64,
    pub maxprot: u32,
    pub initprot: u32,
    pub flags: u32,
    pub sections: Vec<Section<'a>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section<'a> {
    pub sectname: &'a str,
    pub segname: &'a str,
    pub addr: u64,
    pub size: u64,
    pub offset: u32,
    pub align: u32,
    pub reloff: u32,
    pub nreloc: u32,
    pub flags: u32,
    pub reserved1: u32,
    pub reserved2: u32,
    pub reserved3: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Symtab {
    pub symoff: u32,
    pub nsyms: u32,
    pub stroff: u32,
    pub strsize: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Dysymtab {
    pub ilocalsym: u32,
    pub nlocalsym: u32,
    pub iextdefsym: u32,
    pub nextdefsym: u32,
    pub iundefsym: u32,
    pub nundefsym: u32,
    pub tocoff: u32,
    pub ntoc: u32,
    pub modtaboff: u32,
    pub nmodtab: u32,
    pub extrefsymoff: u32,
    pub nextrefsyms: u32,
    pub indirectsymoff: u32,
    pub nindirectsyms: u32,
    pub extreloff: u32,
    pub nextrel: u32,
    pub locreloff: u32,
    pub nlocrel: u32,
}

/// `LC_LOAD_DYLIB` / `LC_ID_DYLIB` / `LC_REEXPORT_DYLIB` /
/// `LC_LOAD_UPWARD_DYLIB` / `LC_LOAD_WEAK_DYLIB` (distinguished by
/// `cmd`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dylib<'a> {
    pub cmd: u32,
    pub name: &'a str,
    pub timestamp: u32,
    pub current_version: u32,
    pub compatibility_version: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dylinker<'a> {
    pub name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryPoint {
    pub entryoff: u64,
    pub stacksize: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Thread<'a> {
    pub flavor: u32,
    pub count: u32,
    pub state: &'a [u8],
}

/// `LC_CODE_SIGNATURE`, `LC_FUNCTION_STARTS`, `LC_DATA_IN_CODE`,
/// `LC_DYLD_CHAINED_FIXUPS`, `LC_DYLD_EXPORTS_TRIE`,
/// `LC_DYLIB_CODE_SIGN_DRS`, `LC_LINKER_OPTIMIZATION_HINT`,
/// `LC_SEGMENT_SPLIT_INFO`, `LC_DYLD_ENVIRONMENT`(no: lc_str),
/// `LC_TWOLEVEL_HINTS`(different shape) — uniform linkedit-data shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkeditData {
    pub cmd: u32,
    pub dataoff: u32,
    pub datasize: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uuid {
    pub uuid: [u8; 16],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildVersion {
    pub platform: u32,
    pub minos: u32,
    pub sdk: u32,
    pub tools: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceVersion {
    pub version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rpath<'a> {
    pub path: &'a str,
}

/// `LC_SUB_*`: umbrella/client/library/framework names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubName<'a> {
    pub cmd: u32,
    pub name: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncryptionInfo {
    pub cryptoff: u32,
    pub cryptsize: u32,
    pub cryptid: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Routines {
    pub init_address: u64,
    pub init_module: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilesetEntry<'a> {
    pub vmaddr: u64,
    pub fileoff: u64,
    pub entry_id: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DyldInfo {
    pub rebase_off: u32,
    pub rebase_size: u32,
    pub bind_off: u32,
    pub bind_size: u32,
    pub weak_bind_off: u32,
    pub weak_bind_size: u32,
    pub lazy_bind_off: u32,
    pub lazy_bind_size: u32,
    pub export_off: u32,
    pub export_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DyldEnvironment<'a> {
    pub value: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkerOption<'a> {
    pub count: u32,
    pub options: Vec<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionMin {
    pub cmd: u32,
    pub version: u32,
    pub sdk: u32,
}

/// Fully decoded load command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadCommand<'a> {
    Segment(Segment<'a>),
    Symtab(Symtab),
    Dysymtab(Dysymtab),
    Dylib(Dylib<'a>),
    Dylinker(Dylinker<'a>),
    EntryPoint(EntryPoint),
    Thread(Thread<'a>),
    LinkeditData(LinkeditData),
    Uuid(Uuid),
    BuildVersion(BuildVersion),
    SourceVersion(SourceVersion),
    Rpath(Rpath<'a>),
    Sub(SubName<'a>),
    EncryptionInfo(EncryptionInfo),
    Routines(Routines),
    FilesetEntry(FilesetEntry<'a>),
    DyldInfo(DyldInfo),
    DyldEnvironment(DyldEnvironment<'a>),
    LinkerOption(LinkerOption<'a>),
    VersionMin(VersionMin),
    /// Preserved verbatim for forward compatibility.
    Unknown {
        cmd: u32,
        cmdsize: u32,
        bytes: &'a [u8],
    },
}

/// Decode `X.Y.Z` packed versions (`LC_BUILD_VERSION`, dylib versions).
pub fn decode_version(packed: u32) -> (u16, u8, u8) {
    (
        (packed >> 16) as u16,
        ((packed >> 8) & 0xff) as u8,
        (packed & 0xff) as u8,
    )
}

pub fn platform_name(platform: u32) -> &'static str {
    match platform {
        PLATFORM_MACOS => "macOS",
        PLATFORM_IOS => "iOS",
        PLATFORM_TVOS => "tvOS",
        PLATFORM_WATCHOS => "watchOS",
        PLATFORM_MACCATALYST => "MacCatalyst",
        PLATFORM_IOSSIMULATOR => "iOSSimulator",
        _ => "unknown",
    }
}

/// `lc_str` lookup: offset relative to the command start, NUL-bounded
/// by the command size.
fn lc_str<'a>(raw: &RawCommand<'a>, at: usize) -> Result<&'a str> {
    let cur = Cursor::new(raw.bytes);
    let str_off = cur.u32le(at)? as usize;
    if str_off >= raw.bytes.len() {
        return Err(ParseError {
            offset: raw.offset + at,
            kind: ErrorKind::StringOutOfRange,
        });
    }
    let bytes = Cursor::new(raw.bytes).cstr(str_off, raw.bytes.len() - str_off)?;
    std::str::from_utf8(bytes).map_err(|_| ParseError {
        offset: raw.offset + str_off,
        kind: ErrorKind::InvalidName,
    })
}

fn decode_segment<'a>(raw: &RawCommand<'a>) -> Result<Segment<'a>> {
    let cur = Cursor::new(raw.bytes);
    // segment_command_64 fixed part is 72 bytes.
    if raw.bytes.len() < 72 {
        return Err(ParseError {
            offset: raw.offset,
            kind: ErrorKind::Truncated {
                need: raw.offset + 72,
                len: raw.offset + raw.bytes.len(),
            },
        });
    }
    let nsects = cur.u32le(64)? as usize;
    let want = 72usize
        .checked_add(nsects.checked_mul(80).ok_or(ParseError {
            offset: raw.offset,
            kind: ErrorKind::OffsetOverflow,
        })?)
        .ok_or(ParseError {
            offset: raw.offset,
            kind: ErrorKind::OffsetOverflow,
        })?;
    if raw.bytes.len() < want {
        return Err(ParseError {
            offset: raw.offset,
            kind: ErrorKind::Truncated {
                need: raw.offset + want,
                len: raw.offset + raw.bytes.len(),
            },
        });
    }
    let mut sections = Vec::with_capacity(nsects.min(128));
    for i in 0..nsects {
        let s = 72 + i * 80;
        sections.push(Section {
            sectname: cur.name(s, 16)?,
            segname: cur.name(s + 16, 16)?,
            addr: cur.u64le(s + 32)?,
            size: cur.u64le(s + 40)?,
            offset: cur.u32le(s + 48)?,
            align: cur.u32le(s + 52)?,
            reloff: cur.u32le(s + 56)?,
            nreloc: cur.u32le(s + 60)?,
            flags: cur.u32le(s + 64)?,
            reserved1: cur.u32le(s + 68)?,
            reserved2: cur.u32le(s + 72)?,
            reserved3: cur.u32le(s + 76)?,
        });
    }
    Ok(Segment {
        segname: cur.name(8, 16)?,
        vmaddr: cur.u64le(24)?,
        vmsize: cur.u64le(32)?,
        fileoff: cur.u64le(40)?,
        filesize: cur.u64le(48)?,
        maxprot: cur.u32le(56)?,
        initprot: cur.u32le(60)?,
        flags: cur.u32le(68)?,
        sections,
    })
}

/// Decode one raw command into its typed form.
pub fn decode<'a>(raw: &RawCommand<'a>) -> Result<LoadCommand<'a>> {
    let cur = Cursor::new(raw.bytes);
    match raw.cmd {
        LC_SEGMENT_64 => Ok(LoadCommand::Segment(decode_segment(raw)?)),
        LC_SYMTAB => Ok(LoadCommand::Symtab(Symtab {
            symoff: cur.u32le(8)?,
            nsyms: cur.u32le(12)?,
            stroff: cur.u32le(16)?,
            strsize: cur.u32le(20)?,
        })),
        LC_DYSYMTAB => {
            let mut w = [0u32; 18];
            for (i, slot) in w.iter_mut().enumerate() {
                *slot = cur.u32le(8 + i * 4)?;
            }
            Ok(LoadCommand::Dysymtab(Dysymtab {
                ilocalsym: w[0],
                nlocalsym: w[1],
                iextdefsym: w[2],
                nextdefsym: w[3],
                iundefsym: w[4],
                nundefsym: w[5],
                tocoff: w[6],
                ntoc: w[7],
                modtaboff: w[8],
                nmodtab: w[9],
                extrefsymoff: w[10],
                nextrefsyms: w[11],
                indirectsymoff: w[12],
                nindirectsyms: w[13],
                extreloff: w[14],
                nextrel: w[15],
                locreloff: w[16],
                nlocrel: w[17],
            }))
        }
        LC_LOAD_DYLIB | LC_ID_DYLIB | LC_REEXPORT_DYLIB | LC_LOAD_UPWARD_DYLIB
        | LC_LOAD_WEAK_DYLIB => Ok(LoadCommand::Dylib(Dylib {
            cmd: raw.cmd,
            name: lc_str(raw, 8)?,
            timestamp: cur.u32le(12)?,
            current_version: cur.u32le(16)?,
            compatibility_version: cur.u32le(20)?,
        })),
        LC_LOAD_DYLINKER => Ok(LoadCommand::Dylinker(Dylinker {
            name: lc_str(raw, 8)?,
        })),
        LC_MAIN => Ok(LoadCommand::EntryPoint(EntryPoint {
            entryoff: cur.u64le(8)?,
            stacksize: cur.u64le(16)?,
        })),
        LC_UNIXTHREAD => Ok(LoadCommand::Thread(Thread {
            flavor: cur.u32le(8)?,
            count: cur.u32le(12)?,
            state: cur.bytes(16, raw.bytes.len().saturating_sub(16))?,
        })),
        LC_CODE_SIGNATURE
        | LC_FUNCTION_STARTS
        | LC_DATA_IN_CODE
        | LC_DYLD_CHAINED_FIXUPS
        | LC_DYLD_EXPORTS_TRIE
        | LC_DYLIB_CODE_SIGN_DRS
        | LC_LINKER_OPTIMIZATION_HINT
        | LC_SEGMENT_SPLIT_INFO => Ok(LoadCommand::LinkeditData(LinkeditData {
            cmd: raw.cmd,
            dataoff: cur.u32le(8)?,
            datasize: cur.u32le(12)?,
        })),
        LC_UUID => {
            let b = cur.bytes(8, 16)?;
            let mut uuid = [0u8; 16];
            uuid.copy_from_slice(b);
            Ok(LoadCommand::Uuid(Uuid { uuid }))
        }
        LC_BUILD_VERSION => {
            let ntools = cur.u32le(20)? as usize;
            if ntools > 64 {
                return Err(ParseError {
                    offset: raw.offset,
                    kind: ErrorKind::TooManyBuildTools { ntools },
                });
            }
            let want = 24 + ntools * 8;
            if raw.bytes.len() < want {
                return Err(ParseError {
                    offset: raw.offset,
                    kind: ErrorKind::Truncated {
                        need: raw.offset + want,
                        len: raw.offset + raw.bytes.len(),
                    },
                });
            }
            let mut tools = Vec::with_capacity(ntools);
            for i in 0..ntools {
                tools.push((cur.u32le(24 + i * 8)?, cur.u32le(28 + i * 8)?));
            }
            Ok(LoadCommand::BuildVersion(BuildVersion {
                platform: cur.u32le(8)?,
                minos: cur.u32le(12)?,
                sdk: cur.u32le(16)?,
                tools,
            }))
        }
        LC_SOURCE_VERSION => Ok(LoadCommand::SourceVersion(SourceVersion {
            version: cur.u64le(8)?,
        })),
        LC_RPATH => Ok(LoadCommand::Rpath(Rpath {
            path: lc_str(raw, 8)?,
        })),
        LC_SUB_FRAMEWORK | LC_SUB_UMBRELLA | LC_SUB_CLIENT | LC_SUB_LIBRARY => {
            Ok(LoadCommand::Sub(SubName {
                cmd: raw.cmd,
                name: lc_str(raw, 8)?,
            }))
        }
        LC_ENCRYPTION_INFO_64 => Ok(LoadCommand::EncryptionInfo(EncryptionInfo {
            cryptoff: cur.u32le(8)?,
            cryptsize: cur.u32le(12)?,
            cryptid: cur.u32le(16)?,
        })),
        LC_ROUTINES_64 => Ok(LoadCommand::Routines(Routines {
            init_address: cur.u64le(8)?,
            init_module: cur.u64le(16)?,
        })),
        LC_FILESET_ENTRY => Ok(LoadCommand::FilesetEntry(FilesetEntry {
            vmaddr: cur.u64le(8)?,
            fileoff: cur.u64le(16)?,
            entry_id: lc_str(raw, 24)?,
        })),
        LC_DYLD_INFO | LC_DYLD_INFO_ONLY => {
            let mut w = [0u32; 10];
            for (i, slot) in w.iter_mut().enumerate() {
                *slot = cur.u32le(8 + i * 4)?;
            }
            Ok(LoadCommand::DyldInfo(DyldInfo {
                rebase_off: w[0],
                rebase_size: w[1],
                bind_off: w[2],
                bind_size: w[3],
                weak_bind_off: w[4],
                weak_bind_size: w[5],
                lazy_bind_off: w[6],
                lazy_bind_size: w[7],
                export_off: w[8],
                export_size: w[9],
            }))
        }
        LC_DYLD_ENVIRONMENT => Ok(LoadCommand::DyldEnvironment(DyldEnvironment {
            value: lc_str(raw, 8)?,
        })),
        LC_LINKER_OPTION => {
            let count = cur.u32le(8)? as usize;
            if count > 256 {
                return Err(ParseError {
                    offset: raw.offset,
                    kind: ErrorKind::TooManyLinkerOptions { count },
                });
            }
            let mut options = Vec::with_capacity(count.min(16));
            let mut pos = 12usize;
            for _ in 0..count {
                if pos >= raw.bytes.len() {
                    return Err(ParseError {
                        offset: raw.offset,
                        kind: ErrorKind::Truncated {
                            need: raw.offset + pos + 1,
                            len: raw.offset + raw.bytes.len(),
                        },
                    });
                }
                let s = Cursor::new(raw.bytes).cstr(pos, raw.bytes.len() - pos)?;
                pos += s.len() + 1;
                options.push(std::str::from_utf8(s).map_err(|_| ParseError {
                    offset: raw.offset + pos,
                    kind: ErrorKind::InvalidName,
                })?);
            }
            Ok(LoadCommand::LinkerOption(LinkerOption {
                count: count as u32,
                options,
            }))
        }
        LC_VERSION_MIN_MACOSX
        | LC_VERSION_MIN_IPHONEOS
        | LC_VERSION_MIN_TVOS
        | LC_VERSION_MIN_WATCHOS => Ok(LoadCommand::VersionMin(VersionMin {
            cmd: raw.cmd,
            version: cur.u32le(8)?,
            sdk: cur.u32le(12)?,
        })),
        _ => Ok(LoadCommand::Unknown {
            cmd: raw.cmd,
            cmdsize: raw.cmdsize,
            bytes: raw.bytes,
        }),
    }
}

/// Command mnemonic for dumps.
pub fn cmd_name(cmd: u32) -> &'static str {
    match cmd {
        LC_SEGMENT => "LC_SEGMENT",
        LC_UUID => "LC_UUID",
        LC_SYMTAB => "LC_SYMTAB",
        LC_UNIXTHREAD => "LC_UNIXTHREAD",
        LC_DYSYMTAB => "LC_DYSYMTAB",
        LC_LOAD_DYLIB => "LC_LOAD_DYLIB",
        LC_ID_DYLIB => "LC_ID_DYLIB",
        LC_LOAD_DYLINKER => "LC_LOAD_DYLINKER",
        LC_PREBOUND_DYLIB => "LC_PREBOUND_DYLIB",
        LC_ROUTINES_64 => "LC_ROUTINES_64",
        LC_SUB_FRAMEWORK => "LC_SUB_FRAMEWORK",
        LC_SUB_UMBRELLA => "LC_SUB_UMBRELLA",
        LC_SUB_CLIENT => "LC_SUB_CLIENT",
        LC_SUB_LIBRARY => "LC_SUB_LIBRARY",
        LC_SEGMENT_64 => "LC_SEGMENT_64",
        LC_CODE_SIGNATURE => "LC_CODE_SIGNATURE",
        LC_SEGMENT_SPLIT_INFO => "LC_SEGMENT_SPLIT_INFO",
        LC_REEXPORT_DYLIB => "LC_REEXPORT_DYLIB",
        LC_DYLD_INFO => "LC_DYLD_INFO",
        LC_DYLD_INFO_ONLY => "LC_DYLD_INFO_ONLY",
        LC_LOAD_UPWARD_DYLIB => "LC_LOAD_UPWARD_DYLIB",
        LC_VERSION_MIN_MACOSX => "LC_VERSION_MIN_MACOSX",
        LC_VERSION_MIN_IPHONEOS => "LC_VERSION_MIN_IPHONEOS",
        LC_FUNCTION_STARTS => "LC_FUNCTION_STARTS",
        LC_DYLD_ENVIRONMENT => "LC_DYLD_ENVIRONMENT",
        LC_MAIN => "LC_MAIN",
        LC_DATA_IN_CODE => "LC_DATA_IN_CODE",
        LC_SOURCE_VERSION => "LC_SOURCE_VERSION",
        LC_DYLIB_CODE_SIGN_DRS => "LC_DYLIB_CODE_SIGN_DRS",
        LC_ENCRYPTION_INFO_64 => "LC_ENCRYPTION_INFO_64",
        LC_LINKER_OPTION => "LC_LINKER_OPTION",
        LC_LINKER_OPTIMIZATION_HINT => "LC_LINKER_OPTIMIZATION_HINT",
        LC_VERSION_MIN_TVOS => "LC_VERSION_MIN_TVOS",
        LC_VERSION_MIN_WATCHOS => "LC_VERSION_MIN_WATCHOS",
        LC_BUILD_VERSION => "LC_BUILD_VERSION",
        LC_DYLD_EXPORTS_TRIE => "LC_DYLD_EXPORTS_TRIE",
        LC_DYLD_CHAINED_FIXUPS => "LC_DYLD_CHAINED_FIXUPS",
        LC_FILESET_ENTRY => "LC_FILESET_ENTRY",
        LC_LOADFVMLIB => "LC_LOADFVMLIB",
        LC_IDFVMLIB => "LC_IDFVMLIB",
        LC_IDENT => "LC_IDENT",
        LC_FVMFILE => "LC_FVMFILE",
        LC_PREPAGE => "LC_PREPAGE",
        LC_RPATH => "LC_RPATH",
        LC_TWOLEVEL_HINTS => "LC_TWOLEVEL_HINTS",
        LC_LOAD_WEAK_DYLIB => "LC_LOAD_WEAK_DYLIB",
        LC_PREBIND_CKSUM => "LC_PREBIND_CKSUM",
        LC_ID_DYLINKER => "LC_ID_DYLINKER",
        LC_NOTE => "LC_NOTE",
        LC_ATOM_INFO => "LC_ATOM_INFO",
        LC_FUNCTION_VARIANTS => "LC_FUNCTION_VARIANTS",
        LC_FUNCTION_VARIANT_FIXUPS => "LC_FUNCTION_VARIANT_FIXUPS",
        LC_TARGET_TRIPLE => "LC_TARGET_TRIPLE",
        LC_LAZY_LOAD_DYLIB_INFO => "LC_LAZY_LOAD_DYLIB_INFO",
        _ => "LC_UNKNOWN",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::SIZEOF_MACH_HEADER_64;

    fn image_with(cmds: &[Vec<u8>]) -> Vec<u8> {
        let sizeofcmds: usize = cmds.iter().map(Vec::len).sum();
        let mut v = vec![0u8; SIZEOF_MACH_HEADER_64];
        v[16..20].copy_from_slice(&(cmds.len() as u32).to_le_bytes());
        v[20..24].copy_from_slice(&(sizeofcmds as u32).to_le_bytes());
        for c in cmds {
            v.extend_from_slice(c);
        }
        v
    }

    fn cmd(cmd: u32, payload: &[u8]) -> Vec<u8> {
        let size = ((8 + payload.len()).div_ceil(8) * 8) as u32;
        let mut v = Vec::new();
        v.extend_from_slice(&cmd.to_le_bytes());
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(payload);
        v.resize(size as usize, 0);
        v
    }

    fn dylib_cmd(code: u32, name: &str) -> Vec<u8> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&24u32.to_le_bytes()); // str off
        payload.extend_from_slice(&0u32.to_le_bytes()); // timestamp
        payload.extend_from_slice(&0x10000u32.to_le_bytes()); // current 1.0.0
        payload.extend_from_slice(&0x10000u32.to_le_bytes()); // compat
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
        cmd(code, &payload)
    }

    fn segment_cmd() -> Vec<u8> {
        let mut payload = vec![0u8; 64];
        payload[0..6].copy_from_slice(b"__TEXT");
        payload[16..24].copy_from_slice(&0x1000_0000u64.to_le_bytes()); // vmaddr
        payload[24..32].copy_from_slice(&0x4000u64.to_le_bytes()); // vmsize
        payload[40..48].copy_from_slice(&0x4000u64.to_le_bytes()); // filesize
        payload[48..52].copy_from_slice(&5u32.to_le_bytes()); // maxprot r+x
        payload[52..56].copy_from_slice(&5u32.to_le_bytes()); // initprot
        payload[56..60].copy_from_slice(&1u32.to_le_bytes()); // nsects
        let mut sect = vec![0u8; 80];
        sect[0..6].copy_from_slice(b"__text");
        sect[16..22].copy_from_slice(b"__TEXT");
        sect[32..40].copy_from_slice(&0x1000_0f50u64.to_le_bytes());
        sect[40..48].copy_from_slice(&0x60u64.to_le_bytes());
        payload.extend_from_slice(&sect);
        cmd(LC_SEGMENT_64, &payload)
    }

    #[test]
    fn walks_and_decodes_mixed_table() {
        let img = image_with(&[
            segment_cmd(),
            dylib_cmd(LC_LOAD_DYLIB, "/usr/lib/libSystem.B.dylib"),
            cmd(
                LC_MAIN,
                &[0x50, 0x0f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            ),
            cmd(0x99, &[1, 2, 3]),
        ]);
        let table: Vec<RawCommand<'_>> =
            Commands::new(&img, 0, 4, (img.len() - SIZEOF_MACH_HEADER_64) as u32)
                .unwrap()
                .collect::<Result<Vec<_>>>()
                .unwrap();
        assert_eq!(table.len(), 4);
        let decoded: Vec<LoadCommand<'_>> = table
            .iter()
            .map(decode)
            .collect::<Result<Vec<_>>>()
            .unwrap();
        match &decoded[0] {
            LoadCommand::Segment(s) => {
                assert_eq!(s.segname, "__TEXT");
                assert_eq!(s.vmaddr, 0x1000_0000);
                assert_eq!(s.sections.len(), 1);
                assert_eq!(s.sections[0].sectname, "__text");
            }
            other => panic!("expected segment, got {other:?}"),
        }
        match &decoded[1] {
            LoadCommand::Dylib(d) => {
                assert_eq!(d.name, "/usr/lib/libSystem.B.dylib");
                assert_eq!(decode_version(d.current_version), (1, 0, 0));
            }
            other => panic!("expected dylib, got {other:?}"),
        }
        match &decoded[2] {
            LoadCommand::EntryPoint(e) => assert_eq!(e.entryoff, 0x0f50),
            other => panic!("expected entry, got {other:?}"),
        }
        assert!(matches!(decoded[3], LoadCommand::Unknown { cmd: 0x99, .. }));
    }

    #[test]
    fn rejects_overflowing_table() {
        let img = image_with(&[cmd(LC_UUID, &[0u8; 16])]);
        // Lie about the count: the second command runs past the table.
        let res: Result<Vec<RawCommand<'_>>> = Commands::new(&img, 0, 2, 24).unwrap().collect();
        assert!(res.is_err());
    }

    #[test]
    fn hostile_ncmds_fuses_after_first_error() {
        // ncmds = u32::MAX over a one-command table: without fusing,
        // callers that ignore errors would spin ~4B iterations (fuzz).
        let img = image_with(&[cmd(LC_UUID, &[0u8; 16])]);
        let mut it = Commands::new(&img, 0, u32::MAX, 24).unwrap();
        assert!(it.next().unwrap().is_ok());
        assert!(it.next().unwrap().is_err());
        assert!(it.next().is_none());
        assert!(it.next().is_none());
    }

    #[test]
    fn rejects_tiny_cmdsize() {
        let mut img = vec![0u8; SIZEOF_MACH_HEADER_64 + 8];
        img[32..36].copy_from_slice(&LC_UUID.to_le_bytes());
        img[36..40].copy_from_slice(&4u32.to_le_bytes()); // cmdsize < 8
        let mut it = Commands::new(&img, 0, 1, 8).unwrap();
        assert!(matches!(
            it.next().unwrap().unwrap_err().kind,
            ErrorKind::CommandTooSmall { .. }
        ));
    }

    #[test]
    fn unixthread_keeps_state_raw() {
        let mut payload = vec![0u8; 8 + 21 * 8];
        payload[0..4].copy_from_slice(&X86_THREAD_STATE64.to_le_bytes());
        payload[4..8].copy_from_slice(&42u32.to_le_bytes()); // 32-bit words
                                                             // rip is word 16 of x86_THREAD_STATE64.
        payload[8 + 16 * 8..8 + 17 * 8].copy_from_slice(&0x1122_3344u64.to_le_bytes());
        let img = image_with(&[cmd(LC_UNIXTHREAD, &payload)]);
        let table: Vec<RawCommand<'_>> =
            Commands::new(&img, 0, 1, (img.len() - SIZEOF_MACH_HEADER_64) as u32)
                .unwrap()
                .collect::<Result<Vec<_>>>()
                .unwrap();
        match decode(&table[0]).unwrap() {
            LoadCommand::Thread(t) => {
                assert_eq!(t.flavor, X86_THREAD_STATE64);
                assert_eq!(t.state.len(), 21 * 8);
            }
            other => panic!("expected thread, got {other:?}"),
        }
    }

    #[test]
    fn dyld_info_and_build_version_decode() {
        let mut payload = vec![0u8; 40];
        payload[0..4].copy_from_slice(&0x100u32.to_le_bytes());
        payload[4..8].copy_from_slice(&0x20u32.to_le_bytes());
        let mut bv = vec![0u8; 24];
        bv[0..4].copy_from_slice(&PLATFORM_MACOS.to_le_bytes());
        bv[4..8].copy_from_slice(&0x0d0000u32.to_le_bytes());
        bv[8..12].copy_from_slice(&0x0e0000u32.to_le_bytes());
        bv[12..16].copy_from_slice(&1u32.to_le_bytes()); // ntools
        bv[16..20].copy_from_slice(&3u32.to_le_bytes()); // tool
        bv[20..24].copy_from_slice(&0x1234u32.to_le_bytes()); // version
        let img = image_with(&[cmd(LC_DYLD_INFO_ONLY, &payload), cmd(LC_BUILD_VERSION, &bv)]);
        let table: Vec<RawCommand<'_>> =
            Commands::new(&img, 0, 2, (img.len() - SIZEOF_MACH_HEADER_64) as u32)
                .unwrap()
                .collect::<Result<Vec<_>>>()
                .unwrap();
        match decode(&table[0]).unwrap() {
            LoadCommand::DyldInfo(d) => {
                assert_eq!((d.rebase_off, d.rebase_size), (0x100, 0x20));
                assert_eq!(d.export_size, 0);
            }
            other => panic!("expected dyld info, got {other:?}"),
        }
        match decode(&table[1]).unwrap() {
            LoadCommand::BuildVersion(b) => {
                assert_eq!(platform_name(b.platform), "macOS");
                assert_eq!(decode_version(b.minos), (13, 0, 0));
                assert_eq!(b.tools, vec![(3, 0x1234)]);
            }
            other => panic!("expected build version, got {other:?}"),
        }
    }
}
