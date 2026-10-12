// SPDX-License-Identifier: MIT OR Apache-2.0
//! Guest execution tests: `mol-loader run` on synthetic same-ISA
//! guests (P1.1.1). Linux-only; CI runs these natively on both arches.
#![cfg(target_os = "linux")]

use std::process::{Command, Output};

// Realistic macOS-style addresses: mapping below vm.mmap_min_addr
// (64K) fails with EPERM for non-root users (CI runners).
const TEXT_VMADDR: u64 = 0x1000_0000;
const DATA_VMADDR: u64 = 0x1000_4000;
const MARK_OFF: u64 = 0x30;

fn host_cpu() -> (u32, u32) {
    #[cfg(target_arch = "aarch64")]
    {
        (0x0100_000c, 0)
    }
    #[cfg(target_arch = "x86_64")]
    {
        (0x0100_0007, 3)
    }
}

fn other_cpu() -> (u32, u32) {
    #[cfg(target_arch = "aarch64")]
    {
        (0x0100_0007, 3)
    }
    #[cfg(target_arch = "x86_64")]
    {
        (0x0100_000c, 0)
    }
}

#[cfg(target_arch = "aarch64")]
fn w32(v: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// exit(42), slide-independent.
fn code_exit42() -> Vec<u8> {
    let mut v = Vec::new();
    #[cfg(target_arch = "aarch64")]
    {
        w32(0x5280_0540, &mut v); // movz w0, #42
        w32(0xd65f_03c0, &mut v); // ret
    }
    #[cfg(target_arch = "x86_64")]
    {
        v.extend_from_slice(&[0xb8, 0x2a, 0x00, 0x00, 0x00, 0xc3]);
    }
    v
}

/// exit(argc): proves the Darwin stack layout end to end.
fn code_argc() -> Vec<u8> {
    let mut v = Vec::new();
    #[cfg(target_arch = "aarch64")]
    {
        // w0 already holds argc at LC_MAIN entry.
        w32(0xd65f_03c0, &mut v); // ret
    }
    #[cfg(target_arch = "x86_64")]
    {
        v.extend_from_slice(&[0x8b, 0xc7, 0xc3]); // mov eax, edi; ret
    }
    v
}

/// Materialize a 64-bit absolute address in `reg` (movz + movk
/// chain, minimal parts).
#[cfg(target_arch = "aarch64")]
fn mov_abs(reg: u32, val: u64, out: &mut Vec<u8>) {
    let parts = [
        (val & 0xffff) as u16,
        ((val >> 16) & 0xffff) as u16,
        ((val >> 32) & 0xffff) as u16,
        ((val >> 48) & 0xffff) as u16,
    ];
    let first = parts.iter().position(|&p| p != 0).unwrap_or(0);
    w32(
        0xd280_0000 | ((first as u32) << 21) | ((parts[first] as u32) << 5) | reg,
        out,
    );
    for (hw, &p) in parts.iter().enumerate() {
        if hw != first && p != 0 {
            w32(
                0xf280_0000 | ((hw as u32) << 21) | ((p as u32) << 5) | reg,
                out,
            );
        }
    }
}

/// Store 7 to the marker. `at` is this routine's own vmaddr (x86_64
/// RIP-relative addressing); `marker` is the slid marker address.
fn code_store(at: u64, marker: u64) -> Vec<u8> {
    let mut v = Vec::new();
    #[cfg(target_arch = "aarch64")]
    {
        mov_abs(0, marker, &mut v);
        w32(0x5280_00e1, &mut v); // mov w1, #7
        w32(0xb900_0001, &mut v); // str w1, [x0]
        w32(0xd65f_03c0, &mut v); // ret
        let _ = at;
    }
    #[cfg(target_arch = "x86_64")]
    {
        let disp = marker.wrapping_sub(at + 10) as u32;
        v.extend_from_slice(&[0xc7, 0x05]);
        v.extend_from_slice(&disp.to_le_bytes());
        v.extend_from_slice(&[0x07, 0x00, 0x00, 0x00]);
        v.push(0xc3); // ret
    }
    v
}

/// exit(marker): `at` is this entry's own vmaddr (x86_64 RIP-rel).
fn code_load(at: u64, marker: u64) -> Vec<u8> {
    let mut v = Vec::new();
    #[cfg(target_arch = "aarch64")]
    {
        mov_abs(0, marker, &mut v);
        w32(0xb940_0000, &mut v); // ldr w0, [x0]
        w32(0xd65f_03c0, &mut v); // ret
        let _ = at;
    }
    #[cfg(target_arch = "x86_64")]
    {
        let disp = marker.wrapping_sub(at + 6) as u32;
        v.extend_from_slice(&[0x8b, 0x05]);
        v.extend_from_slice(&disp.to_le_bytes());
        v.push(0xc3); // ret
    }
    v
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

fn segment(name: &str, vmaddr: u64, fileoff: u64, filesize: u64, prot: u32) -> Vec<u8> {
    let mut p = vec![0u8; 64];
    p[..name.len()].copy_from_slice(name.as_bytes());
    p[16..24].copy_from_slice(&vmaddr.to_le_bytes());
    p[24..32].copy_from_slice(&filesize.to_le_bytes());
    p[32..40].copy_from_slice(&fileoff.to_le_bytes());
    p[40..48].copy_from_slice(&filesize.to_le_bytes());
    p[48..52].copy_from_slice(&7u32.to_le_bytes());
    p[52..56].copy_from_slice(&prot.to_le_bytes());
    cmd(0x19, &p)
}

/// Minimal executable: `__TEXT` (headers + code) + `__DATA` (marker
/// page) + `LC_MAIN` + optional `LC_ROUTINES_64`.
fn build_guest(cputype: u32, cpusubtype: u32, entry: &[u8], routine: Option<&[u8]>) -> Vec<u8> {
    let mut cmds = vec![
        segment("__TEXT", TEXT_VMADDR, 0, 0, 5),
        segment("__DATA", DATA_VMADDR, 0, 0x1000, 3),
    ];
    // entryoff patched after layout.
    cmds.push(cmd(0x8000_0028, &[0u8; 16]));
    if routine.is_some() {
        cmds.push(cmd(0x1a, &[0u8; 56]));
    }
    let cmds_len: usize = cmds.iter().map(Vec::len).sum();
    let code_off = 32 + cmds_len;
    let entry_len = entry.len();
    let routine_off = code_off + entry_len;
    let data_off =
        (code_off + entry_len + routine.map(|r| r.len()).unwrap_or(0)).next_multiple_of(0x1000);
    // Patch: TEXT filesize, DATA fileoff, LC_MAIN entryoff, ROUTINES addr.
    let text_size = (data_off + 0x1000) as u64;
    cmds[0][32..40].copy_from_slice(&text_size.to_le_bytes());
    cmds[0][48..56].copy_from_slice(&text_size.to_le_bytes());
    cmds[1][40..48].copy_from_slice(&(data_off as u64).to_le_bytes());
    cmds[2][8..16].copy_from_slice(&(code_off as u64).to_le_bytes());
    if routine.is_some() {
        let addr = TEXT_VMADDR + routine_off as u64;
        cmds[3][8..16].copy_from_slice(&addr.to_le_bytes());
    }
    let mut file = vec![0u8; 32];
    file[..4].copy_from_slice(&0xfeed_facfu32.to_le_bytes());
    file[4..8].copy_from_slice(&cputype.to_le_bytes());
    file[8..12].copy_from_slice(&cpusubtype.to_le_bytes());
    file[12..16].copy_from_slice(&2u32.to_le_bytes()); // MH_EXECUTE
    file[16..20].copy_from_slice(&(cmds.len() as u32).to_le_bytes());
    file[20..24].copy_from_slice(&(cmds_len as u32).to_le_bytes());
    file[24..28].copy_from_slice(&0x2000_0085u32.to_le_bytes()); // PIE, twolevel...
    for c in &cmds {
        file.extend_from_slice(c);
    }
    file.extend_from_slice(entry);
    if let Some(r) = routine {
        file.extend_from_slice(r);
    }
    file.resize(data_off, 0);
    file.resize(data_off + 0x1000, 0);
    assert_eq!(file.len(), data_off + 0x1000);
    file
}

fn write_guest(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("mol-guest-{}-{name}", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

fn run(path: &std::path::Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mol-loader"))
        .arg("run")
        .args(extra)
        .arg(path)
        .output()
        .expect("spawn mol-loader")
}

#[test]
fn guest_exit42_fixed_slide() {
    let img = build_guest(host_cpu().0, host_cpu().1, &code_exit42(), None);
    let path = write_guest("exit42", &img);
    let out = run(&path, &["--slide", "0"]);
    assert_eq!(
        out.status.code(),
        Some(42),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn guest_exit42_random_slide() {
    let img = build_guest(host_cpu().0, host_cpu().1, &code_exit42(), None);
    let path = write_guest("exit42r", &img);
    let out = run(&path, &[]);
    assert_eq!(
        out.status.code(),
        Some(42),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn guest_exit42_big_slide() {
    let img = build_guest(host_cpu().0, host_cpu().1, &code_exit42(), None);
    let path = write_guest("exit42b", &img);
    let out = run(&path, &["--slide", "0x300000"]);
    assert_eq!(
        out.status.code(),
        Some(42),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn guest_init_runs_before_entry() {
    // Routine stores 7 at the marker; entry exits with the marker.
    // Slide 0: addresses are hardcoded in the code. Layout is
    // deterministic: cmds are TEXT(72) + DATA(72) + MAIN(24) +
    // ROUTINES(64) = 232 bytes, entry first, routine right after.
    let marker = DATA_VMADDR + MARK_OFF;
    let code_off = 32 + 232;
    let te = TEXT_VMADDR + code_off as u64;
    // Entry length differs per arch; compute routine addr after.
    let entry = code_load(te, marker);
    let tr = TEXT_VMADDR + code_off as u64 + entry.len() as u64;
    let routine = code_store(tr, marker);
    let img = build_guest(host_cpu().0, host_cpu().1, &entry, Some(&routine));
    let path = write_guest("initmark", &img);
    let out = run(&path, &["--slide", "0"]);
    assert_eq!(
        out.status.code(),
        Some(7),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn guest_sees_argc() {
    let img = build_guest(host_cpu().0, host_cpu().1, &code_argc(), None);
    let path = write_guest("argc", &img);
    let out = Command::new(env!("CARGO_BIN_EXE_mol-loader"))
        .arg("run")
        .args(["--slide", "0"])
        .arg(&path)
        .args(["a", "bb", "ccc"])
        .output()
        .expect("spawn mol-loader");
    // argv[0] + 3 args.
    assert_eq!(
        out.status.code(),
        Some(4),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn cross_arch_fails_clean() {
    let img = build_guest(other_cpu().0, other_cpu().1, &code_exit42(), None);
    let path = write_guest("cross", &img);
    let out = run(&path, &[]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("cannot run on") && err.contains("P6"),
        "stderr: {err}"
    );
    std::fs::remove_file(&path).ok();
}
