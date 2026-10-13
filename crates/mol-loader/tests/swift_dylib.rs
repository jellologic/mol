// SPDX-License-Identifier: MIT OR Apache-2.0
//! Swift-shaped dylib load + init end-to-end (P1.3.2): a base-0
//! dylib carrying `__swift5_proto`, a Swift class with `+load`, an
//! `LC_ROUTINES_64` init, and a Swift-mangled export loads at a
//! random slide; init runs first (S), then the Swift `+load` (W),
//! then main calls the export (F) and exits (E): stdout `SWFE`.
//! Linux-only; CI runs natively on both arches.
#![cfg(target_os = "linux")]

mod guestimg;

use std::process::Command;

use guestimg::*;

const MANGLED_FOO: &str = "$s6libdep3fooyyF";

const D_TEXT: u64 = 0;
const D_DATA: u64 = 0x4000;
const D_CODE_OFF: usize = 0x1000;
const D_DATA_OFF: usize = 0x4000;

fn host_cpu() -> (u32, u32) {
    #[cfg(target_arch = "aarch64")]
    {
        (CPU_TYPE_ARM64, 0)
    }
    #[cfg(target_arch = "x86_64")]
    {
        (CPU_TYPE_X86_64, CPU_SUBTYPE_X86_64_ALL)
    }
}

fn dsite(c: &[u8]) -> u64 {
    D_TEXT + D_CODE_OFF as u64 + c.len() as u64
}

fn site(c: &[u8]) -> u64 {
    TEXT_VMADDR + CODE_OFF + c.len() as u64
}

/// `write(1, marker, 1)` + `ret`. `got` = the image's GOT base.
#[cfg(target_arch = "aarch64")]
fn emit_write_ret(c: &mut Vec<u8>, marker: u64, got: u64, at: fn(&[u8]) -> u64) {
    a_stp_frame(c);
    a_movz(c, 0, 1, 0, false);
    let s = at(c);
    a_adr(c, 1, s, marker);
    a_movz(c, 2, 1, 0, false);
    let s = at(c);
    a_call_got(c, s, got);
    a_ldp_frame(c);
    a_ret(c);
}

/// `write(1, marker, 1)` + `ret`. `got` = the image's GOT base.
#[cfg(target_arch = "x86_64")]
fn emit_write_ret(c: &mut Vec<u8>, marker: u64, got: u64, at: fn(&[u8]) -> u64) {
    x_sub_rsp_8(c);
    x_mov_rdi_imm32(c, 1);
    let s = at(c);
    x_lea_rip(c, 6, s, marker);
    x_mov_r32_imm(c, 2, 1);
    let s = at(c);
    x_call_got(c, s, got);
    x_add_rsp_8(c);
    x_ret(c);
}

fn w64_at(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

fn method_list(load_va: u64, imp_va: u64, list_va: u64) -> Vec<u8> {
    let mut v = vec![0u8; 20];
    v[0..4].copy_from_slice(&0x8000_000cu32.to_le_bytes());
    v[4..8].copy_from_slice(&1u32.to_le_bytes());
    let entry = list_va + 8;
    let rel = |t: u64| i32::try_from(t as i64 - entry as i64).unwrap() as u32;
    v[8..12].copy_from_slice(&rel(load_va).to_le_bytes());
    v[12..16].copy_from_slice(&rel(load_va).to_le_bytes());
    v[16..20].copy_from_slice(&rel(imp_va).to_le_bytes());
    v
}

/// Synthetic Swift runtime dylib: swift5 section, Swift class with
/// `+load`, routines init, one mangled export. Link base 0 (real
/// dylib convention); loaded at a random slide.
fn build_swift_dylib(cputype: u32, cpusubtype: u32, id: &str) -> Vec<u8> {
    // DATA floor plan (offsets from D_DATA_OFF).
    let got_len = 8usize; // _write only
    let sw_ro = got_len;
    let msw_ro = sw_ro + 72;
    let sw_c = msw_ro + 72;
    let msw_c = sw_c + 40;
    let classlist_o = msw_c + 40;
    let markers_o = classlist_o + 8;
    let data_len = markers_o + 3;
    let dva = |o: usize| D_DATA + o as u64;

    // Code: init (S), foo export (F), Swift +load IMP (W).
    let mut code = Vec::new();
    let init_va = D_TEXT + D_CODE_OFF as u64;
    emit_write_ret(&mut code, dva(markers_o), D_DATA, dsite);
    let foo_va = D_TEXT + D_CODE_OFF as u64 + code.len() as u64;
    emit_write_ret(&mut code, dva(markers_o + 1), D_DATA, dsite);
    let imp_va = D_TEXT + D_CODE_OFF as u64 + code.len() as u64;
    emit_write_ret(&mut code, dva(markers_o + 2), D_DATA, dsite);

    // TEXT tables.
    let mut text = code;
    let meth_off = D_CODE_OFF + text.len();
    text.extend_from_slice(b"load\0");
    let load_va = D_TEXT + meth_off as u64;
    let name_off = D_CODE_OFF + text.len();
    text.extend_from_slice(b"Swifty\0");
    let cls_name = D_TEXT + name_off as u64;
    while text.len() % 8 != 0 {
        text.push(0);
    }
    let const_off = D_CODE_OFF + text.len();
    let list_va = D_TEXT + const_off as u64;
    text.extend_from_slice(&method_list(load_va, imp_va, list_va));
    let proto_off = D_CODE_OFF + text.len();
    text.extend_from_slice(b"SWIFT5PROTO\x00\x00\x00\x00\x00");
    assert!(D_CODE_OFF + text.len() <= 0x4000, "text overflow");
    let code_len = meth_off - D_CODE_OFF;

    // DATA with rebase-site tracking (absolute link VAs).
    let mut data = vec![0u8; data_len];
    let mut rebases: Vec<RebaseSite> = Vec::new();
    let mut ptr = |data: &mut Vec<u8>, off: usize, v: u64| {
        rebases.push(RebaseSite {
            off: off as u64,
            value: v,
        });
    };
    w64_at(&mut data, sw_ro + 24, cls_name);
    ptr(&mut data, sw_ro + 24, cls_name);
    w64_at(&mut data, msw_ro + 24, cls_name);
    ptr(&mut data, msw_ro + 24, cls_name);
    w64_at(&mut data, msw_ro + 32, list_va);
    ptr(&mut data, msw_ro + 32, list_va);
    w64_at(&mut data, sw_c, dva(msw_c));
    ptr(&mut data, sw_c, dva(msw_c));
    w64_at(&mut data, sw_c + 32, dva(sw_ro));
    ptr(&mut data, sw_c + 32, dva(sw_ro));
    w64_at(&mut data, msw_c, dva(msw_c));
    ptr(&mut data, msw_c, dva(msw_c));
    w64_at(&mut data, msw_c + 32, dva(msw_ro));
    ptr(&mut data, msw_c + 32, dva(msw_ro));
    w64_at(&mut data, classlist_o, dva(sw_c));
    ptr(&mut data, classlist_o, dva(sw_c));
    data[markers_o..markers_o + 3].copy_from_slice(b"SFW");

    // Chained fixups: 1 bind + 8 rebases, one merged chain.
    let imports = [("_write", 1u32)];
    let (chained, patches) = chained_payload_full(&imports, &rebases);
    for (off, enc) in &patches {
        w64_at(&mut data, *off as usize, *enc);
    }
    let trie = exports_trie(&[(MANGLED_FOO, foo_va)]);

    // Commands.
    let mut cmds = vec![
        segment_sections(
            "__TEXT",
            D_TEXT,
            0,
            0x4000,
            0x4000,
            5,
            &[
                SectionSpec {
                    sectname: "__text",
                    segname: "__TEXT",
                    addr: D_TEXT + D_CODE_OFF as u64,
                    size: code_len as u64,
                    offset: D_CODE_OFF as u32,
                },
                SectionSpec {
                    sectname: "__objc_methname",
                    segname: "__TEXT",
                    addr: load_va,
                    size: 5,
                    offset: meth_off as u32,
                },
                SectionSpec {
                    sectname: "__objc_classname",
                    segname: "__TEXT",
                    addr: cls_name,
                    size: 7,
                    offset: name_off as u32,
                },
                SectionSpec {
                    sectname: "__objc_const",
                    segname: "__TEXT",
                    addr: list_va,
                    size: 20,
                    offset: const_off as u32,
                },
                SectionSpec {
                    sectname: "__swift5_proto",
                    segname: "__TEXT",
                    addr: D_TEXT + proto_off as u64,
                    size: 16,
                    offset: proto_off as u32,
                },
            ],
        ),
        segment_sections(
            "__DATA",
            D_DATA,
            D_DATA_OFF as u64,
            data_len as u64,
            0x1000,
            3,
            &[
                SectionSpec {
                    sectname: "__objc_data",
                    segname: "__DATA",
                    addr: dva(got_len),
                    size: (classlist_o - got_len) as u64,
                    offset: (D_DATA_OFF + got_len) as u32,
                },
                SectionSpec {
                    sectname: "__objc_classlist",
                    segname: "__DATA",
                    addr: dva(classlist_o),
                    size: 8,
                    offset: (D_DATA_OFF + classlist_o) as u32,
                },
            ],
        ),
        id_cmd(id),
        dylib_cmd(SHIM_PATH),
        routines_cmd(init_va),
    ];
    let trie_off = CHAIN_OFF as usize + chained.len();
    let mut tri = vec![0u8; 8];
    tri[0..4].copy_from_slice(&(trie_off as u32).to_le_bytes());
    tri[4..8].copy_from_slice(&(trie.len() as u32).to_le_bytes());
    cmds.push(cmd(LC_DYLD_EXPORTS_TRIE, &tri));
    let mut fix = vec![0u8; 8];
    fix[0..4].copy_from_slice(&(CHAIN_OFF as u32).to_le_bytes());
    fix[4..8].copy_from_slice(&(chained.len() as u32).to_le_bytes());
    cmds.push(cmd(LC_DYLD_CHAINED_FIXUPS, &fix));
    let cmds_len: usize = cmds.iter().map(Vec::len).sum();
    assert!(32 + cmds_len <= CHAIN_OFF as usize, "cmds overflow chain");

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
    file.resize(CHAIN_OFF as usize, 0);
    file.extend_from_slice(&chained);
    file.extend_from_slice(&trie);
    file.resize(D_CODE_OFF, 0);
    file.extend_from_slice(&text);
    file.resize(0x4000, 0);
    file.extend_from_slice(&data);
    file.resize(0x4000 + data_len, 0);
    file
}

/// Main: calls the dylib export (F), writes E, exits 0.
#[cfg(target_arch = "aarch64")]
fn main_code(marker_e: u64) -> Vec<u8> {
    let mut c = vec![];
    let s = site(&c);
    a_call_got(&mut c, s, got_slot(2));
    a_movz(&mut c, 0, 1, 0, false);
    let s = site(&c);
    a_adr(&mut c, 1, s, marker_e);
    a_movz(&mut c, 2, 1, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, got_slot(0));
    a_movz(&mut c, 0, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, got_slot(1));
    c
}

/// Main: calls the dylib export (F), writes E, exits 0.
#[cfg(target_arch = "x86_64")]
fn main_code(marker_e: u64) -> Vec<u8> {
    let mut c = vec![];
    x_sub_rsp_8(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, got_slot(2));
    x_mov_rdi_imm32(&mut c, 1);
    let s = site(&c);
    x_lea_rip(&mut c, 6, s, marker_e);
    x_mov_r32_imm(&mut c, 2, 1);
    let s = site(&c);
    x_call_got(&mut c, s, got_slot(0));
    x_mov_rdi_imm32(&mut c, 0);
    let s = site(&c);
    x_call_got(&mut c, s, got_slot(1));
    c
}

#[test]
fn swift_dylib_loads_and_initializes() {
    let (ct, st) = host_cpu();
    let tag = std::process::id();
    let dep_path = std::env::temp_dir().join(format!("mol-p13-{tag}-libswift.dylib"));
    let main_path = std::env::temp_dir().join(format!("mol-p13-{tag}-main"));
    let dep = build_swift_dylib(ct, st, &dep_path.to_string_lossy());
    std::fs::write(&dep_path, &dep).unwrap();
    let dep_str = dep_path.to_string_lossy().into_owned();
    // GOT has 3 slots; marker E lives in main DATA after them.
    let marker_e = DATA_VMADDR + 24;
    let code = main_code(marker_e);
    let main = build_import_guest_full(
        ct,
        st,
        &[SHIM_PATH, &dep_str],
        &[("_write", 1), ("_exit", 1), (MANGLED_FOO, 2)],
        &code,
        b"E",
    );
    std::fs::write(&main_path, &main).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mol-loader"))
        .arg("run")
        .arg("--slide")
        .arg("0")
        .arg("--debug-catch-faults")
        .arg(&main_path)
        .output()
        .expect("spawn mol-loader");
    eprintln!("DEP {} MAIN {}", dep_path.display(), main_path.display());
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"SWFE");
}
