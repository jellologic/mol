// SPDX-License-Identifier: MIT OR Apache-2.0
//! `libSystem.B.dylib` shim generator (P1.2.2).
//!
//! Guests link `/usr/lib/libSystem.B.dylib`; mol serves this
//! generated image (in-memory overlay, materializable to the prefix
//! per the P1.1.4 real-files strategy). Each export is a trampoline
//! stub in `__text` that loads its target from a `__data` slot the
//! loader fills with the handler address — so dyld binds resolve
//! normally and all host knowledge stays in the loader. No chained
//! fixups (host fills slots) and no symtab (trie suffices for
//! `dyld_info -exports`).
//!
//! Layout per slice: linked at zero (`__TEXT` stubs at file
//! `0x400`, `__DATA` slots at file `0x4000`, trie at file `0x8000`
//! in `__LINKEDIT`); segments 64K-spaced in VA (see below).
//! Deterministic: the same registry always emits identical bytes
//! (golden-tested).

/// Install path guests link.
pub const SHIM_PATH: &str = "/usr/lib/libSystem.B.dylib";

// Linked at zero like a real dylib (dyld_info validates export
// offsets against a zero base); the loader slides it at map time.
// Segments are 64K-spaced in VA (files stay compact): adjacent
// segments must never share a host page, or per-segment mprotect
// conflicts on 16K/64K-page kernels. Real Apple dylibs pack
// 4K-spaced (Apple has no 64K user pages); the spacing is mol's
// documented deviation.
pub const TEXT_VMADDR: u64 = 0x0;
pub const DATA_VMADDR: u64 = 0x1_0000;
pub const LINKEDIT_VMADDR: u64 = 0x2_0000;
const STUB_OFF: u64 = 0x400;
const SLOT_OFF: u64 = 0x4000;
const TRIE_OFF: u64 = 0x8000;
const SLICE_SIZE: u64 = 0x9000;

const MH_MAGIC_64: u32 = 0xfeed_facf;
const MH_DYLIB: u32 = 6;
const CPU_TYPE_ARM64: u32 = 0x0100_000c;
const CPU_TYPE_X86_64: u32 = 0x0100_0007;
const CPU_SUBTYPE_ARM64_ALL: u32 = 0;
const CPU_SUBTYPE_X86_64_ALL: u32 = 3;
const LC_SEGMENT_64: u32 = 0x19;
const LC_ID_DYLIB: u32 = 0xd;
const LC_DYLD_EXPORTS_TRIE: u32 = 0x8000_0033;
const FAT_MAGIC: u32 = 0xcafe_babe;

/// Shim export kind: code stub vs data slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    /// `__text` stub jumping through the slot.
    Func,
    /// `__data` slot holding the value itself (the stack-guard).
    Data,
}

/// One shim export: name + kind. Addresses derive from the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    pub name: &'static str,
    pub kind: ExportKind,
}

/// Guest slice architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Arm64,
    X86_64,
}

impl Arch {
    fn stub_len(self) -> u64 {
        match self {
            Arch::Arm64 => 12,
            Arch::X86_64 => 6,
        }
    }
}

/// Slot address for export `index` (both arches share the layout).
pub fn slot_addr(index: usize) -> u64 {
    DATA_VMADDR + index as u64 * 8
}

/// Export addresses in registry order: funcs pack densely from the
/// stub base (data exports punch no holes — they have no stub),
/// data exports address their slot.
pub fn packed_addrs(exports: &[Export], arch: Arch) -> Vec<u64> {
    let mut pack = 0u64;
    exports
        .iter()
        .enumerate()
        .map(|(i, e)| match e.kind {
            ExportKind::Func => {
                let a = TEXT_VMADDR + STUB_OFF + pack;
                pack += arch.stub_len();
                a
            }
            ExportKind::Data => slot_addr(i),
        })
        .collect()
}

fn uleb(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v >>= 7;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

fn uleb_len(mut v: u64) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

/// Build an export trie: flat root, one leaf per symbol. Names must
/// be pre-sorted. Child offsets use minimal ulebs via fixpoint.
fn build_trie(exports: &[(String, u64)]) -> Vec<u8> {
    // Leaves first (sizes are fixpoint-independent).
    let mut leaves: Vec<Vec<u8>> = Vec::new();
    for (_, addr) in exports {
        let mut term = Vec::new();
        uleb(0, &mut term); // flags
        uleb(*addr, &mut term);
        let mut leaf = Vec::new();
        leaf.push(term.len() as u8);
        leaf.extend_from_slice(&term);
        leaf.push(0); // no children
        leaves.push(leaf);
    }
    // Root skeleton to fixpoint.
    let count = exports.len();
    assert!(count < 256);
    let mut widths = vec![1usize; count];
    let root_len = |widths: &[usize]| {
        2 + exports
            .iter()
            .zip(widths.iter())
            .map(|((name, _), w)| name.len() + 1 + *w)
            .sum::<usize>()
    };
    loop {
        let base = root_len(&widths);
        let mut off = base;
        let mut changed = false;
        for (i, leaf) in leaves.iter().enumerate() {
            let need = uleb_len(off as u64);
            if need != widths[i] {
                widths[i] = need;
                changed = true;
            }
            off += leaf.len();
        }
        if !changed {
            break;
        }
    }
    let mut trie = vec![0u8, count as u8];
    let mut off = root_len(&widths);
    for (i, ((name, _), leaf)) in exports.iter().zip(leaves.iter()).enumerate() {
        trie.extend_from_slice(name.as_bytes());
        trie.push(0);
        // Pad the uleb to the fixpoint width (uleb allows trailing
        // zero groups: value-preserving).
        let mut enc = Vec::new();
        uleb(off as u64, &mut enc);
        while enc.len() < widths[i] {
            let last = enc.len() - 1;
            enc[last] |= 0x80;
            enc.push(0);
        }
        trie.extend_from_slice(&enc);
        off += leaf.len();
    }
    for leaf in &leaves {
        trie.extend_from_slice(leaf);
    }
    trie
}

/// arm64 stub: `adrp x16, slot_page; ldr x16, [x16, #off]; br x16`.
fn arm64_stub(stub: u64, slot: u64) -> [u8; 12] {
    let stub_page = stub & !0xfff;
    let slot_page = slot & !0xfff;
    let delta = ((slot_page as i64 - stub_page as i64) >> 12) as i32 as u32;
    let adrp = 0x9000_0010 | ((delta & 3) << 29) | (((delta >> 2) & 0x7_ffff) << 5);
    let off = (slot & 0xfff) as u32;
    assert!(off % 8 == 0 && off < 0x1000);
    let ldr = 0xf940_0000 | ((off >> 3) << 10) | (16 << 5) | 16;
    let br: u32 = 0xd61f_0000 | (16 << 5);
    let mut b = [0u8; 12];
    b[0..4].copy_from_slice(&adrp.to_le_bytes());
    b[4..8].copy_from_slice(&ldr.to_le_bytes());
    b[8..12].copy_from_slice(&br.to_le_bytes());
    b
}

/// x86_64 stub: `jmp *slot(%rip)`.
fn x86_64_stub(stub: u64, slot: u64) -> [u8; 6] {
    let disp = slot.wrapping_sub(stub + 6) as i32;
    let mut b = [0u8; 6];
    b[0] = 0xff;
    b[1] = 0x25;
    b[2..6].copy_from_slice(&disp.to_le_bytes());
    b
}

fn cmd(code: u32, payload: &[u8]) -> Vec<u8> {
    let size = ((8 + payload.len()).div_ceil(8) * 8) as u32;
    let mut v = Vec::new();
    v.extend_from_slice(&code.to_le_bytes());
    v.extend_from_slice(&size.to_le_bytes());
    v.extend_from_slice(payload);
    v.resize(size as usize, 0);
    v
}

#[allow(clippy::too_many_arguments)]
fn section(
    sect: &str,
    seg: &str,
    addr: u64,
    size: u64,
    offset: u32,
    align: u32,
    flags: u32,
) -> Vec<u8> {
    let mut s = vec![0u8; 80];
    s[..sect.len()].copy_from_slice(sect.as_bytes());
    s[16..16 + seg.len()].copy_from_slice(seg.as_bytes());
    s[32..40].copy_from_slice(&addr.to_le_bytes());
    s[40..48].copy_from_slice(&size.to_le_bytes());
    s[48..52].copy_from_slice(&offset.to_le_bytes());
    s[52..56].copy_from_slice(&align.to_le_bytes());
    s[64..68].copy_from_slice(&flags.to_le_bytes());
    s
}

fn segment(
    name: &str,
    vmaddr: u64,
    fileoff: u64,
    filesize: u64,
    vmsize: u64,
    initprot: u32,
    sects: &[Vec<u8>],
) -> Vec<u8> {
    // Body after (cmd, cmdsize) is 64 bytes (struct
    // segment_command_64 is 72 total); sections follow immediately.
    let mut p = vec![0u8; 64];
    p[..name.len()].copy_from_slice(name.as_bytes());
    p[16..24].copy_from_slice(&vmaddr.to_le_bytes());
    p[24..32].copy_from_slice(&vmsize.to_le_bytes());
    p[32..40].copy_from_slice(&fileoff.to_le_bytes());
    p[40..48].copy_from_slice(&filesize.to_le_bytes());
    p[48..52].copy_from_slice(&7u32.to_le_bytes());
    p[52..56].copy_from_slice(&initprot.to_le_bytes());
    p[56..60].copy_from_slice(&(sects.len() as u32).to_le_bytes());
    for s in sects {
        p.extend_from_slice(s);
    }
    cmd(LC_SEGMENT_64, &p)
}

fn dylib_cmd(code: u32, name: &str) -> Vec<u8> {
    let mut payload = vec![0u8; 16];
    payload[0..4].copy_from_slice(&24u32.to_le_bytes());
    payload.extend_from_slice(name.as_bytes());
    payload.push(0);
    cmd(code, &payload)
}

/// Build one thin slice for `arch` over the export registry.
fn build_slice(exports: &[Export], arch: Arch) -> Vec<u8> {
    let (cputype, cpusubtype) = match arch {
        Arch::Arm64 => (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL),
        Arch::X86_64 => (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL),
    };
    let addrs = packed_addrs(exports, arch);
    let mut stubs = Vec::new();
    for (i, e) in exports.iter().enumerate() {
        if e.kind == ExportKind::Func {
            let slot = slot_addr(i);
            match arch {
                Arch::Arm64 => stubs.extend_from_slice(&arm64_stub(addrs[i], slot)),
                Arch::X86_64 => stubs.extend_from_slice(&x86_64_stub(addrs[i], slot)),
            }
        }
    }
    let mut trie_in: Vec<(String, u64)> = exports
        .iter()
        .enumerate()
        .map(|(i, e)| (e.name.to_string(), addrs[i]))
        .collect();
    trie_in.sort_by(|a, b| a.0.cmp(&b.0));
    let trie = build_trie(&trie_in);

    let sect_text = section(
        "__text",
        "__TEXT",
        TEXT_VMADDR + STUB_OFF,
        stubs.len() as u64,
        STUB_OFF as u32,
        2,
        0x8000_0400,
    );
    let sect_data = section(
        "__data",
        "__DATA",
        DATA_VMADDR,
        exports.len() as u64 * 8,
        SLOT_OFF as u32,
        3,
        0,
    );
    let mut cmds = vec![
        segment("__TEXT", TEXT_VMADDR, 0, 0x4000, 0x1_0000, 5, &[sect_text]),
        segment(
            "__DATA",
            DATA_VMADDR,
            SLOT_OFF,
            0x4000,
            0x1_0000,
            3,
            &[sect_data],
        ),
        segment(
            "__LINKEDIT",
            LINKEDIT_VMADDR,
            TRIE_OFF,
            0x1000,
            0x1_0000,
            1,
            &[],
        ),
        dylib_cmd(LC_ID_DYLIB, SHIM_PATH),
        cmd(LC_DYLD_EXPORTS_TRIE, &[0u8; 8]),
    ];
    let cmds_len: usize = cmds.iter().map(Vec::len).sum();
    assert!(
        32 + cmds_len <= STUB_OFF as usize,
        "commands overflow stub area"
    );
    assert!(trie.len() <= 0x1000, "trie overflows linkedit page");
    let blob = cmds.last_mut().unwrap();
    blob[8..12].copy_from_slice(&(TRIE_OFF as u32).to_le_bytes());
    blob[12..16].copy_from_slice(&(trie.len() as u32).to_le_bytes());
    let mut file = vec![0u8; 32];
    file[..4].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
    file[4..8].copy_from_slice(&cputype.to_le_bytes());
    file[8..12].copy_from_slice(&cpusubtype.to_le_bytes());
    file[12..16].copy_from_slice(&MH_DYLIB.to_le_bytes());
    file[16..20].copy_from_slice(&(cmds.len() as u32).to_le_bytes());
    file[20..24].copy_from_slice(&(cmds_len as u32).to_le_bytes());
    for c in &cmds {
        file.extend_from_slice(c);
    }
    file.resize(STUB_OFF as usize, 0);
    file.extend_from_slice(&stubs);
    file.resize(SLOT_OFF as usize, 0);
    // Slots start zero; the loader fills them (data exports read the
    // value, func stubs jump through it).
    file.resize((SLOT_OFF + exports.len() as u64 * 8) as usize, 0);
    file.resize(TRIE_OFF as usize, 0);
    file.extend_from_slice(&trie);
    file.resize(SLICE_SIZE as usize, 0);
    file
}

/// Build the fat shim dylib (arm64 + x86_64 slices).
pub fn build_shim(exports: &[Export]) -> Vec<u8> {
    let arm64 = build_slice(exports, Arch::Arm64);
    let x86_64 = build_slice(exports, Arch::X86_64);
    assert_eq!(arm64.len() as u64, SLICE_SIZE);
    assert_eq!(x86_64.len() as u64, SLICE_SIZE);
    // Slices at 16K boundaries (macOS arm64 page size; dyld_info
    // rejects 4K-aligned slices as "not page aligned").
    let arm_off = 0x4000u32;
    let x64_off = (arm_off + SLICE_SIZE as u32).next_multiple_of(0x4000);
    let mut fat = Vec::new();
    fat.extend_from_slice(&FAT_MAGIC.to_be_bytes());
    fat.extend_from_slice(&2u32.to_be_bytes());
    for (cputype, cpusubtype, off) in [
        (CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL, arm_off),
        (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL, x64_off),
    ] {
        fat.extend_from_slice(&cputype.to_be_bytes());
        fat.extend_from_slice(&cpusubtype.to_be_bytes());
        fat.extend_from_slice(&off.to_be_bytes());
        fat.extend_from_slice(&(SLICE_SIZE as u32).to_be_bytes());
        fat.extend_from_slice(&14u32.to_be_bytes()); // 2^14 align
    }
    fat.resize(arm_off as usize, 0);
    fat.extend_from_slice(&arm64);
    fat.resize(x64_off as usize, 0);
    fat.extend_from_slice(&x86_64);
    fat
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Vec<Export> {
        crate::exports()
    }

    #[test]
    fn registry_unique_and_guarded() {
        let reg = registry();
        assert!(!reg.is_empty());
        let names: Vec<&str> = reg.iter().map(|e| e.name).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(names.len(), sorted.len(), "duplicate export names");
        // Exactly one data export: the stack guard.
        let data: Vec<&&str> = reg
            .iter()
            .filter(|e| e.kind == ExportKind::Data)
            .map(|e| &e.name)
            .collect();
        assert_eq!(data, vec![&"___stack_chk_guard"]);
    }

    #[test]
    fn build_is_deterministic() {
        let reg = registry();
        assert_eq!(build_shim(&reg), build_shim(&reg));
    }

    #[test]
    fn slices_parse_with_expected_exports() {
        let reg = registry();
        let fat = build_shim(&reg);
        let file = mol_macho::parse(&fat).unwrap();
        let mol_macho::MachFile::Fat(f) = &file else {
            panic!("shim is not fat");
        };
        assert_eq!(f.fat.arches.len(), 2);
        for (arch, want_cpu, want_sub) in [
            (Arch::Arm64, CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL),
            (Arch::X86_64, CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL),
        ] {
            let file = mol_macho::parse(&fat).unwrap();
            let img = file.select(want_cpu, want_sub).unwrap();
            let map = img.load_map(0).unwrap();
            assert_eq!(map.dylibs.len(), 1);
            // Sections parse at the right offsets (Apple tools
            // caught an 8-byte slip here once).
            let text = map.segments.iter().find(|s| s.segname == "__TEXT").unwrap();
            assert_eq!(text.sections.len(), 1);
            assert_eq!(text.sections[0].sectname, "__text");
            assert_eq!(text.sections[0].addr, TEXT_VMADDR + STUB_OFF);
            assert_eq!(text.sections[0].offset, STUB_OFF as u32);
            let data = map.segments.iter().find(|s| s.segname == "__DATA").unwrap();
            assert_eq!(data.sections.len(), 1);
            assert_eq!(data.sections[0].sectname, "__data");
            assert_eq!(data.sections[0].addr, DATA_VMADDR);
            assert_eq!(data.sections[0].offset, SLOT_OFF as u32);
            let want = packed_addrs(&reg, arch);
            for (i, e) in reg.iter().enumerate() {
                let got = map
                    .exports
                    .iter()
                    .find(|x| x.name == e.name)
                    .unwrap_or_else(|| panic!("{} missing {arch:?} export {}", SHIM_PATH, e.name));
                assert_eq!(got.address, want[i], "export {} ({arch:?})", e.name);
            }
        }
    }

    #[test]
    fn stubs_decode_to_their_slots() {
        let reg = registry();
        let fat = build_shim(&reg);
        for (arch, want_cpu, want_sub) in [
            (Arch::Arm64, CPU_TYPE_ARM64, CPU_SUBTYPE_ARM64_ALL),
            (Arch::X86_64, CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL),
        ] {
            let file = mol_macho::parse(&fat).unwrap();
            let img = file.select(want_cpu, want_sub).unwrap();
            let map = img.load_map(0).unwrap();
            let text = map.segments.iter().find(|s| s.segname == "__TEXT").unwrap();
            let addrs = packed_addrs(&reg, arch);
            for (i, e) in reg.iter().enumerate() {
                if e.kind == ExportKind::Data {
                    continue;
                }
                let stub = addrs[i];
                let at = (text.fileoff + (stub - TEXT_VMADDR)) as usize;
                let slot = slot_addr(i);
                match arch {
                    Arch::Arm64 => {
                        let w = |o: usize| {
                            u32::from_le_bytes(img.bytes[at + o..at + o + 4].try_into().unwrap())
                        };
                        // adrp x16, #delta
                        assert_eq!(w(0) & 0x9f00_001f, 0x9000_0010, "export {}", e.name);
                        let delta = (((w(0) >> 5) & 0x7_ffff) << 2) | ((w(0) >> 29) & 3);
                        let got_page = (stub & !0xfff).wrapping_add((delta as u64) << 12);
                        assert_eq!(got_page, slot & !0xfff, "export {}", e.name);
                        // ldr x16, [x16, #off]
                        assert_eq!(w(4) & 0xffc0_03ff, 0xf940_0210, "export {}", e.name);
                        let off = (((w(4) >> 10) & 0xfff) << 3) as u64;
                        assert_eq!(got_page + off, slot, "export {}", e.name);
                        // br x16
                        assert_eq!(w(8), 0xd61f_0200, "export {}", e.name);
                    }
                    Arch::X86_64 => {
                        assert_eq!(img.bytes[at], 0xff, "export {}", e.name);
                        assert_eq!(img.bytes[at + 1], 0x25, "export {}", e.name);
                        let disp =
                            i32::from_le_bytes(img.bytes[at + 2..at + 6].try_into().unwrap());
                        assert_eq!(
                            stub.wrapping_add(6).wrapping_add(disp as u64),
                            slot,
                            "export {}",
                            e.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn golden_bytes_stable() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../corpus/shim/libSystem.B.dylib"
        );
        let golden =
            std::fs::read(path).expect("corpus/shim/libSystem.B.dylib (regen: gen_shim example)");
        assert_eq!(build_shim(&registry()), golden);
    }
}
