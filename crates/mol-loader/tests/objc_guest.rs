// SPDX-License-Identifier: MIT OR Apache-2.0
//! ObjC `+load` end-to-end (P1.3.1): a guest with real `__objc_*`
//! sections (Root/Mid classes scrambled in the classlist, one
//! category) drains Root, Mid, CatMid through the loader before
//! entry prints E. Linux-only; CI runs natively on both arches.
#![cfg(target_os = "linux")]

mod guestimg;

use std::process::Command;

use guestimg::*;

// Data layout (file offsets from DATA_OFF, fixed sizes).
const DATA_OFF: usize = 0x4000;
const GOT_LEN: usize = 16; // _write, _exit
const RO_LEN: usize = 72;
const CLASS_LEN: usize = 40;
const CAT_LEN: usize = 56;

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

fn site(c: &[u8]) -> u64 {
    TEXT_VMADDR + CODE_OFF + c.len() as u64
}

/// `write(1, marker, 1)` + `exit(0)` entry.
#[cfg(target_arch = "aarch64")]
fn entry_code(marker_e: u64) -> Vec<u8> {
    let mut c = vec![];
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

/// `write(1, marker, 1)` + `exit(0)` entry.
#[cfg(target_arch = "x86_64")]
fn entry_code(marker_e: u64) -> Vec<u8> {
    let mut c = vec![];
    x_sub_rsp_8(&mut c);
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

/// `+load` body: `write(1, marker, 1)`; returns to the drain.
/// Saves LR: the inner `bl` would otherwise make `ret` loop.
#[cfg(target_arch = "aarch64")]
fn imp_code(c: &mut Vec<u8>, marker: u64) {
    a_stp_frame(c);
    a_movz(c, 0, 1, 0, false);
    let s = site(c);
    a_adr(c, 1, s, marker);
    a_movz(c, 2, 1, 0, false);
    let s = site(c);
    a_call_got(c, s, got_slot(0));
    a_ldp_frame(c);
    a_ret(c);
}

/// `+load` body: `write(1, marker, 1)`; returns to the drain.
#[cfg(target_arch = "x86_64")]
fn imp_code(c: &mut Vec<u8>, marker: u64) {
    x_sub_rsp_8(c);
    x_mov_rdi_imm32(c, 1);
    let s = site(c);
    x_lea_rip(c, 6, s, marker);
    x_mov_r32_imm(c, 2, 1);
    let s = site(c);
    x_call_got(c, s, got_slot(0));
    x_add_rsp_8(c);
    x_ret(c);
}

fn w64_at(buf: &mut [u8], off: usize, v: u64) {
    buf[off..off + 8].copy_from_slice(&v.to_le_bytes());
}

/// Small one-method `+load` list with relative offsets.
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

fn build_objc_guest(cputype: u32, cpusubtype: u32) -> Vec<u8> {
    // Fixed DATA floor plan (offsets from DATA_OFF).
    let structs_off = GOT_LEN;
    let root_ro = structs_off;
    let mid_ro = root_ro + RO_LEN;
    let mroot_ro = mid_ro + RO_LEN;
    let mmid_ro = mroot_ro + RO_LEN;
    let root_c = mmid_ro + RO_LEN;
    let mroot_c = root_c + CLASS_LEN;
    let mid_c = mroot_c + CLASS_LEN;
    let mmid_c = mid_c + CLASS_LEN;
    let cat_o = mmid_c + CLASS_LEN;
    let structs_len = CAT_LEN + (cat_o - structs_off);
    let classlist_o = structs_off + structs_len;
    let catlist_o = classlist_o + 16;
    let markers_o = catlist_o + 8;
    let data_len = markers_o + 4;
    let dva = |o: usize| DATA_VMADDR + o as u64;

    let imports = ["_write", "_exit"];
    let (chained, got) = chained_payload(&imports);
    assert_eq!(got.len(), GOT_LEN);

    // Code first (IMPs feed the method lists).
    let mut code = entry_code(dva(markers_o + 3));
    let imp_r = TEXT_VMADDR + CODE_OFF + code.len() as u64;
    imp_code(&mut code, dva(markers_o));
    let imp_m = TEXT_VMADDR + CODE_OFF + code.len() as u64;
    imp_code(&mut code, dva(markers_o + 1));
    let imp_c = TEXT_VMADDR + CODE_OFF + code.len() as u64;
    imp_code(&mut code, dva(markers_o + 2));

    // TEXT tables after the code.
    let mut text = code;
    assert!(CODE_OFF as usize + text.len() < 0x2000, "code too large");
    let meth_off = CODE_OFF as usize + text.len();
    text.extend_from_slice(b"load\0");
    let load_va = TEXT_VMADDR + meth_off as u64;
    let name_off = CODE_OFF as usize + text.len();
    text.extend_from_slice(b"Root\0Mid\0CatMid\0");
    let root_name = TEXT_VMADDR + name_off as u64;
    let mid_name = root_name + 5;
    let cat_name = mid_name + 4;
    while text.len() % 8 != 0 {
        text.push(0);
    }
    let const_off = CODE_OFF as usize + text.len();
    let list_r_va = TEXT_VMADDR + const_off as u64;
    text.extend_from_slice(&method_list(load_va, imp_r, list_r_va));
    let list_m_va = TEXT_VMADDR + CODE_OFF + text.len() as u64;
    text.extend_from_slice(&method_list(load_va, imp_m, list_m_va));
    let list_c_va = TEXT_VMADDR + CODE_OFF + text.len() as u64;
    text.extend_from_slice(&method_list(load_va, imp_c, list_c_va));
    assert!(CODE_OFF as usize + text.len() <= 0x4000, "text overflow");
    let code_len = meth_off - CODE_OFF as usize;

    // DATA structs.
    let mut data = vec![0u8; data_len];
    data[..GOT_LEN].copy_from_slice(&got);
    // class_ro ×4 (name + baseMethods on metas).
    w64_at(&mut data, root_ro + 24, root_name);
    w64_at(&mut data, mid_ro + 24, mid_name);
    w64_at(&mut data, mroot_ro + 24, root_name);
    w64_at(&mut data, mroot_ro + 32, list_r_va);
    w64_at(&mut data, mmid_ro + 24, mid_name);
    w64_at(&mut data, mmid_ro + 32, list_m_va);
    // class_t pairs: Root (root), Mid : Root.
    w64_at(&mut data, root_c, dva(mroot_c));
    w64_at(&mut data, root_c + 32, dva(root_ro));
    w64_at(&mut data, mroot_c, dva(mroot_c));
    w64_at(&mut data, mroot_c + 32, dva(mroot_ro));
    w64_at(&mut data, mid_c, dva(mmid_c));
    w64_at(&mut data, mid_c + 8, dva(root_c));
    w64_at(&mut data, mid_c + 32, dva(mid_ro));
    w64_at(&mut data, mmid_c, dva(mmid_c));
    w64_at(&mut data, mmid_c + 32, dva(mmid_ro));
    // Category on Mid with +load.
    w64_at(&mut data, cat_o, cat_name);
    w64_at(&mut data, cat_o + 8, dva(mid_c));
    w64_at(&mut data, cat_o + 24, list_c_va);
    // Scrambled classlist [Mid, Root] + catlist.
    w64_at(&mut data, classlist_o, dva(mid_c));
    w64_at(&mut data, classlist_o + 8, dva(root_c));
    w64_at(&mut data, catlist_o, dva(cat_o));
    data[markers_o..markers_o + 4].copy_from_slice(b"RMCE");

    // Commands.
    let mut cmds = vec![
        segment_sections(
            "__TEXT",
            TEXT_VMADDR,
            0,
            0x4000,
            0x4000,
            5,
            &[
                SectionSpec {
                    sectname: "__text",
                    segname: "__TEXT",
                    addr: TEXT_VMADDR + CODE_OFF,
                    size: code_len as u64,
                    offset: CODE_OFF as u32,
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
                    addr: root_name,
                    size: 16,
                    offset: name_off as u32,
                },
                SectionSpec {
                    sectname: "__objc_const",
                    segname: "__TEXT",
                    addr: list_r_va,
                    size: 60,
                    offset: const_off as u32,
                },
            ],
        ),
        segment_sections(
            "__DATA",
            DATA_VMADDR,
            DATA_OFF as u64,
            data_len as u64,
            0x1000,
            3,
            &[
                SectionSpec {
                    sectname: "__objc_data",
                    segname: "__DATA",
                    addr: dva(structs_off),
                    size: structs_len as u64,
                    offset: (DATA_OFF + structs_off) as u32,
                },
                SectionSpec {
                    sectname: "__objc_classlist",
                    segname: "__DATA",
                    addr: dva(classlist_o),
                    size: 16,
                    offset: (DATA_OFF + classlist_o) as u32,
                },
                SectionSpec {
                    sectname: "__objc_catlist",
                    segname: "__DATA",
                    addr: dva(catlist_o),
                    size: 8,
                    offset: (DATA_OFF + catlist_o) as u32,
                },
            ],
        ),
        dylib_cmd(SHIM_PATH),
    ];
    let mut fix = vec![0u8; 8];
    fix[0..4].copy_from_slice(&(CHAIN_OFF as u32).to_le_bytes());
    fix[4..8].copy_from_slice(&(chained.len() as u32).to_le_bytes());
    cmds.push(cmd(LC_DYLD_CHAINED_FIXUPS, &fix));
    let mut main = vec![0u8; 16];
    main[0..8].copy_from_slice(&CODE_OFF.to_le_bytes());
    cmds.push(cmd(LC_MAIN, &main));
    let cmds_len: usize = cmds.iter().map(Vec::len).sum();
    assert!(32 + cmds_len <= CHAIN_OFF as usize, "cmds overflow chain");

    let mut file = vec![0u8; 32];
    file[..4].copy_from_slice(&MH_MAGIC_64.to_le_bytes());
    file[4..8].copy_from_slice(&cputype.to_le_bytes());
    file[8..12].copy_from_slice(&cpusubtype.to_le_bytes());
    file[12..16].copy_from_slice(&MH_EXECUTE.to_le_bytes());
    file[16..20].copy_from_slice(&(cmds.len() as u32).to_le_bytes());
    file[20..24].copy_from_slice(&(cmds_len as u32).to_le_bytes());
    file[24..28].copy_from_slice(&MH_PIE.to_le_bytes());
    for c in &cmds {
        file.extend_from_slice(c);
    }
    file.resize(CHAIN_OFF as usize, 0);
    file.extend_from_slice(&chained);
    file.resize(CODE_OFF as usize, 0);
    file.extend_from_slice(&text);
    file.resize(0x4000, 0);
    file.extend_from_slice(&data);
    file.resize(0x4000 + data_len, 0);
    file
}

#[test]
fn objc_load_order_rmce() {
    let (ct, st) = host_cpu();
    let img = build_objc_guest(ct, st);
    let path = std::env::temp_dir().join(format!("mol-p13-{}", std::process::id()));
    std::fs::write(&path, &img).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_mol-loader"))
        .arg("run")
        .arg("--slide")
        .arg("0")
        .arg("--debug-catch-faults")
        .arg(&path)
        .output()
        .expect("spawn mol-loader");
    std::fs::remove_file(&path).ok();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"RMCE");
}
