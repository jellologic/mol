// SPDX-License-Identifier: MIT OR Apache-2.0
//! Guest end-to-end: Mach-O guests exercising libSystem shims (P1.2).
//! Each guest is built by `guestimg`, runs under `mol-loader run` at
//! slide 0, and self-checks; failed checks exit with distinct codes
//! (10+). Linux-only; CI runs these natively on both arches.
#![cfg(target_os = "linux")]

#[path = "guestimg.rs"]
mod guestimg;

use guestimg::*;
use std::process::{Command, Output};

const CODE_VA: u64 = TEXT_VMADDR + CODE_OFF;
const GOT_VA: u64 = DATA_VMADDR;

fn data_va(nimports: usize) -> u64 {
    GOT_VA + 8 * nimports as u64
}

fn slot(i: usize) -> u64 {
    GOT_VA + 8 * i as u64
}

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

fn write_guest(name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("mol-p12-{}-{name}", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

fn run(path: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mol-loader"))
        .arg("run")
        .arg("--slide")
        .arg("0")
        // Fault diagnostics only; silent when green.
        .arg("--debug-catch-faults")
        .arg(path)
        .output()
        .expect("spawn mol-loader")
}

/// Human exit/signal summary for failure messages (a bare `None`
/// code hides which signal killed the guest).
fn status_str(out: &Output) -> String {
    use std::os::unix::process::ExitStatusExt;
    match out.status.code() {
        Some(c) => format!("exit {c}"),
        None => format!("signal {:?}", out.status.signal()),
    }
}

fn site(c: &[u8]) -> u64 {
    CODE_VA + c.len() as u64
}

// Forward-branch placeholders: emit `b.<ok>` over an inline fail
// epilogue, then patch once the target is laid out.
#[cfg(target_arch = "aarch64")]
struct Fix {
    pos: usize,
    cond: u32,
}

#[cfg(target_arch = "aarch64")]
fn b_ok(c: &mut Vec<u8>, cond: u32) -> Fix {
    let pos = c.len();
    let s = site(c);
    a_b_cond(c, cond, s, s);
    Fix { pos, cond }
}

#[cfg(target_arch = "aarch64")]
fn b_patch(c: &mut Vec<u8>, f: Fix) {
    let target = site(c);
    patch_u32(c, f.pos, enc_b_cond(f.cond, CODE_VA + f.pos as u64, target));
}

/// Inline `exit(code)` epilogue (arm64); falls through to the ok path.
#[cfg(target_arch = "aarch64")]
fn epilogue(c: &mut Vec<u8>, code: u16, exit_idx: usize) {
    a_movz(c, 0, code, 0, false);
    let s = site(c);
    a_call_got(c, s, slot(exit_idx));
}

#[cfg(target_arch = "x86_64")]
struct Fix {
    pos: usize,
}

#[cfg(target_arch = "x86_64")]
fn j_ok(c: &mut Vec<u8>, op: u8) -> Fix {
    let pos = c.len();
    let s = site(c);
    x_jcc(c, op, s, s);
    Fix { pos }
}

#[cfg(target_arch = "x86_64")]
fn j_patch(c: &mut Vec<u8>, f: Fix) {
    let target = site(c);
    patch_jcc(c, f.pos, CODE_VA + f.pos as u64, target);
}

/// Inline `exit(code)` epilogue (x86_64); falls through to the ok path.
#[cfg(target_arch = "x86_64")]
fn epilogue(c: &mut Vec<u8>, code: u32, exit_idx: usize) {
    x_mov_rdi_imm32(c, code);
    let s = site(c);
    x_call_got(c, s, slot(exit_idx));
}

// ---------------------------------------------------------------------------
// T1: write + exit.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
fn t1_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    let s = site(&c);
    a_adr(&mut c, 1, s, dv);
    a_movz(&mut c, 0, 1, 0, false);
    a_movz(&mut c, 2, 6, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(0));
    a_movz(&mut c, 0, 7, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(1));
    c
}

#[cfg(target_arch = "x86_64")]
fn t1_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    x_mov_rdi_imm32(&mut c, 1);
    let s = site(&c);
    x_lea_rip(&mut c, 6, s, dv);
    x_mov_r32_imm(&mut c, 2, 6);
    let s = site(&c);
    x_call_got(&mut c, s, slot(0));
    x_mov_rdi_imm32(&mut c, 7);
    let s = site(&c);
    x_call_got(&mut c, s, slot(1));
    c
}

fn t1_image() -> Vec<u8> {
    let (ct, st) = host_cpu();
    let dv = data_va(2);
    build_import_guest(ct, st, &["_write", "_exit"], &t1_code(dv), b"hello\n")
}

#[test]
fn t1_write_stdout_exit7() {
    let path = write_guest("t1", &t1_image());
    let out = run(&path);
    assert_eq!(
        out.status.code(),
        Some(7),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
    assert_eq!(out.stdout, b"hello\n");
}

// ---------------------------------------------------------------------------
// T2: errno names (open fails; guest observes ENOENT and exits with it).
// `_strerror` is outside the P1.2.2 smoke set, so the errno number
// itself is the observable: exit code 2 == ENOENT on Darwin.
// ---------------------------------------------------------------------------

fn t2_data() -> Vec<u8> {
    let mut d = b"/nonexistent-mol-guest-path".to_vec();
    d.push(0);
    d
}

#[cfg(target_arch = "aarch64")]
fn t2_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    // open(path, O_RDONLY) must return -1.
    let s = site(&c);
    a_adr(&mut c, 0, s, dv);
    a_movz(&mut c, 1, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(0));
    // open returns int: -1 arrives as 0xFFFF_FFFF in w0 (compare W!).
    a_cmn_w1(&mut c, 0);
    let ok = b_ok(&mut c, 0); // eq: is -1
    epilogue(&mut c, 10, 2);
    b_patch(&mut c, ok);
    // errno must be ENOENT (2); exit with it.
    let s = site(&c);
    a_call_got(&mut c, s, slot(1));
    a_ldr_w(&mut c, 0, 0);
    a_cmp_w_imm(&mut c, 0, 2);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 11, 2);
    b_patch(&mut c, ok);
    // w0 == 2 here: exit(errno).
    let s = site(&c);
    a_call_got(&mut c, s, slot(2));
    c
}

#[cfg(target_arch = "x86_64")]
fn t2_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    let s = site(&c);
    x_lea_rip(&mut c, 7, s, dv);
    x_xor_esi(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(0));
    x_cmp_eax_imm32(&mut c, 0xffff_ffff);
    let ok = j_ok(&mut c, 0x84); // je: is -1
    epilogue(&mut c, 10, 2);
    j_patch(&mut c, ok);
    let s = site(&c);
    x_call_got(&mut c, s, slot(1));
    x_load_eax_rax(&mut c);
    x_cmp_eax_imm32(&mut c, 2);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 11, 2);
    j_patch(&mut c, ok);
    // eax == 2 here: exit(errno).
    x_mov_edi_eax(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(2));
    c
}

#[test]
fn t2_errno_enoent_exit_code() {
    let (ct, st) = host_cpu();
    let dv = data_va(3);
    let img = build_import_guest(
        ct,
        st,
        &["_open", "___error", "_exit"],
        &t2_code(dv),
        &t2_data(),
    );
    let path = write_guest("t2", &img);
    let out = run(&path);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
}

// ---------------------------------------------------------------------------
// T3: open/read/close round-trip (content echoed to stdout).
// ---------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
fn t3_code(dv: u64, buf: u64) -> Vec<u8> {
    let mut c = vec![];
    // open(path, O_RDONLY) must succeed.
    let s = site(&c);
    a_adr(&mut c, 0, s, dv);
    a_movz(&mut c, 1, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(0));
    a_subs_x_imm(&mut c, 31, 0, 0); // cmp x0, #0
    let ok = b_ok(&mut c, 10); // ge
    epilogue(&mut c, 10, 4);
    b_patch(&mut c, ok);
    a_mov_w(&mut c, 19, 0); // save fd
                            // read(fd, buf, 64) must return 11.
    a_mov_w(&mut c, 0, 19);
    let s = site(&c);
    a_adr(&mut c, 1, s, buf);
    a_movz(&mut c, 2, 64, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(1));
    a_cmp_w_imm(&mut c, 0, 11);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 11, 4);
    b_patch(&mut c, ok);
    // Echo the bytes to stdout; the test asserts content.
    a_movz(&mut c, 0, 1, 0, false);
    let s = site(&c);
    a_adr(&mut c, 1, s, buf);
    a_movz(&mut c, 2, 11, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(2));
    // close(fd) must return 0.
    a_mov_w(&mut c, 0, 19);
    let s = site(&c);
    a_call_got(&mut c, s, slot(3));
    a_subs_x_imm(&mut c, 31, 0, 0);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 12, 4);
    b_patch(&mut c, ok);
    a_movz(&mut c, 0, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(4));
    c
}

#[cfg(target_arch = "x86_64")]
fn t3_code(dv: u64, buf: u64) -> Vec<u8> {
    let mut c = vec![];
    let s = site(&c);
    x_lea_rip(&mut c, 7, s, dv);
    x_xor_esi(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(0));
    x_cmp_eax_0(&mut c);
    let ok = j_ok(&mut c, 0x8d); // jge
    epilogue(&mut c, 10, 4);
    j_patch(&mut c, ok);
    x_mov_ebx_eax(&mut c);
    x_mov_edi_ebx(&mut c);
    let s = site(&c);
    x_lea_rip(&mut c, 6, s, buf);
    x_mov_r32_imm(&mut c, 2, 64);
    let s = site(&c);
    x_call_got(&mut c, s, slot(1));
    x_cmp_eax_imm32(&mut c, 11);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 11, 4);
    j_patch(&mut c, ok);
    x_mov_rdi_imm32(&mut c, 1);
    let s = site(&c);
    x_lea_rip(&mut c, 6, s, buf);
    x_mov_r32_imm(&mut c, 2, 11);
    let s = site(&c);
    x_call_got(&mut c, s, slot(2));
    x_mov_edi_ebx(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(3));
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 12, 4);
    j_patch(&mut c, ok);
    x_mov_rdi_imm32(&mut c, 0);
    let s = site(&c);
    x_call_got(&mut c, s, slot(4));
    c
}

#[test]
fn t3_open_read_close_echo() {
    let content = b"hello world";
    let src = std::env::temp_dir().join(format!("mol-p12-t3-{}", std::process::id()));
    std::fs::write(&src, content).unwrap();
    let mut data = src.to_str().unwrap().as_bytes().to_vec();
    data.push(0);
    let buf_off = data.len() as u64;
    data.resize(data.len() + 64, 0);
    let (ct, st) = host_cpu();
    let dv = data_va(5);
    let img = build_import_guest(
        ct,
        st,
        &["_open", "_read", "_write", "_close", "_exit"],
        &t3_code(dv, dv + buf_off),
        &data,
    );
    let path = write_guest("t3", &img);
    let out = run(&path);
    std::fs::remove_file(&src).ok();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
    assert_eq!(out.stdout, content);
}

// ---------------------------------------------------------------------------
// T4: mmap/munmap anonymous (first byte echoed to stdout).
// ---------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
fn t4_code() -> Vec<u8> {
    let mut c = vec![];
    // mmap(0, 0x1000, PROT_READ|PROT_WRITE, MAP_PRIVATE|MAP_ANON, -1, 0).
    a_movz(&mut c, 0, 0, 0, true);
    a_movz(&mut c, 1, 0x1000, 0, false);
    a_movz(&mut c, 2, 3, 0, false);
    a_movz(&mut c, 3, 0x1002, 0, false);
    a_movn_x0(&mut c, 4);
    a_movz(&mut c, 5, 0, 0, true);
    let s = site(&c);
    a_call_got(&mut c, s, slot(0));
    a_cmn_x1(&mut c, 0);
    let ok = b_ok(&mut c, 1); // ne: not MAP_FAILED
    epilogue(&mut c, 10, 3);
    b_patch(&mut c, ok);
    a_mov_x(&mut c, 19, 0);
    // Store a marker word; page is fresh-zero so low byte reads 0xAB.
    a_movz(&mut c, 1, 0xab, 0, false);
    a_str_w(&mut c, 1, 19);
    // write(1, addr, 16): test asserts stdout[0] == 0xAB.
    a_movz(&mut c, 0, 1, 0, false);
    a_mov_x(&mut c, 1, 19);
    a_movz(&mut c, 2, 16, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(1));
    // munmap(addr, 0x1000) must return 0.
    a_mov_x(&mut c, 0, 19);
    a_movz(&mut c, 1, 0x1000, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(2));
    a_subs_x_imm(&mut c, 31, 0, 0);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 11, 3);
    b_patch(&mut c, ok);
    a_movz(&mut c, 0, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(3));
    c
}

#[cfg(target_arch = "x86_64")]
fn t4_code() -> Vec<u8> {
    let mut c = vec![];
    x_xor_edi(&mut c);
    x_mov_esi_imm32(&mut c, 0x1000);
    x_mov_r32_imm(&mut c, 2, 3);
    x_mov_ecx_imm32(&mut c, 0x1002);
    x_mov_r8d_imm32(&mut c, 0xffff_ffff);
    x_xor_r9d(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(0));
    x_cmp_eax_imm32(&mut c, 0xffff_ffff);
    let ok = j_ok(&mut c, 0x85); // jne: not MAP_FAILED
    epilogue(&mut c, 10, 3);
    j_patch(&mut c, ok);
    x_mov_r64_r64(&mut c, 3, 0);
    x_store_rbx_imm32(&mut c, 0xab);
    x_mov_rdi_imm32(&mut c, 1);
    x_mov_r64_r64(&mut c, 6, 3);
    x_mov_r32_imm(&mut c, 2, 16);
    let s = site(&c);
    x_call_got(&mut c, s, slot(1));
    x_mov_rdi_rbx(&mut c);
    x_mov_esi_imm32(&mut c, 0x1000);
    let s = site(&c);
    x_call_got(&mut c, s, slot(2));
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 11, 3);
    j_patch(&mut c, ok);
    x_mov_rdi_imm32(&mut c, 0);
    let s = site(&c);
    x_call_got(&mut c, s, slot(3));
    c
}

#[test]
fn t4_mmap_writeback_munmap() {
    let (ct, st) = host_cpu();
    let img = build_import_guest(
        ct,
        st,
        &["_mmap", "_write", "_munmap", "_exit"],
        &t4_code(),
        &[],
    );
    let path = write_guest("t4", &img);
    let out = run(&path);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
    assert_eq!(out.stdout.len(), 16);
    assert_eq!(out.stdout[0], 0xab);
}

// ---------------------------------------------------------------------------
// T5: sigaction delivery (SIGUSR1 handler sets a marker word).
// ---------------------------------------------------------------------------

const T5_SIG: u16 = 30; // SIGUSR1 on Darwin.
const T5_MARKER: u16 = 7;

#[cfg(target_arch = "aarch64")]
fn t5_code(dv: u64) -> (Vec<u8>, u64) {
    let mut c = vec![];
    let marker = dv + 16;
    // sigaction(SIGUSR1, &act, NULL) must return 0.
    a_movz(&mut c, 0, T5_SIG, 0, false);
    let s = site(&c);
    a_adr(&mut c, 1, s, dv);
    a_movz(&mut c, 2, 0, 0, true);
    let s = site(&c);
    a_call_got(&mut c, s, slot(0));
    a_subs_x_imm(&mut c, 31, 0, 0);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 10, 3);
    b_patch(&mut c, ok);
    // pid = getpid() must be positive.
    let s = site(&c);
    a_call_got(&mut c, s, slot(1));
    a_subs_x_imm(&mut c, 31, 0, 0);
    let ok = b_ok(&mut c, 12); // gt
    epilogue(&mut c, 11, 3);
    b_patch(&mut c, ok);
    a_mov_w(&mut c, 19, 0);
    // kill(pid, SIGUSR1) must return 0.
    a_mov_w(&mut c, 0, 19);
    a_movz(&mut c, 1, T5_SIG, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(2));
    a_subs_x_imm(&mut c, 31, 0, 0);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 12, 3);
    b_patch(&mut c, ok);
    // The handler must have stored the marker.
    let s = site(&c);
    a_adr(&mut c, 0, s, marker);
    a_ldr_w(&mut c, 0, 0);
    a_cmp_w_imm(&mut c, 0, T5_MARKER as u32);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 13, 3);
    b_patch(&mut c, ok);
    a_movz(&mut c, 0, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(3));
    // Handler: marker = 7; return.
    let s = site(&c);
    let handler = s;
    let s = site(&c);
    a_adr(&mut c, 0, s, marker);
    a_movz(&mut c, 1, T5_MARKER, 0, false);
    a_str_w(&mut c, 1, 0);
    a_ret(&mut c);
    (c, handler)
}

#[cfg(target_arch = "x86_64")]
fn t5_code(dv: u64) -> (Vec<u8>, u64) {
    let mut c = vec![];
    let marker = dv + 16;
    x_mov_rdi_imm32(&mut c, T5_SIG as u32);
    let s = site(&c);
    x_lea_rip(&mut c, 6, s, dv);
    x_xor_edx(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(0));
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 10, 3);
    j_patch(&mut c, ok);
    let s = site(&c);
    x_call_got(&mut c, s, slot(1));
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x8f); // jg
    epilogue(&mut c, 11, 3);
    j_patch(&mut c, ok);
    x_mov_ebx_eax(&mut c);
    x_mov_edi_ebx(&mut c);
    x_mov_esi_imm32(&mut c, T5_SIG as u32);
    let s = site(&c);
    x_call_got(&mut c, s, slot(2));
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 12, 3);
    j_patch(&mut c, ok);
    let s = site(&c);
    x_load_eax_rip(&mut c, s, marker);
    x_cmp_eax_imm32(&mut c, T5_MARKER as u32);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 13, 3);
    j_patch(&mut c, ok);
    x_mov_rdi_imm32(&mut c, 0);
    let s = site(&c);
    x_call_got(&mut c, s, slot(3));
    let s = site(&c);
    let handler = s;
    let s = site(&c);
    x_store_rip_imm32(&mut c, s, marker, T5_MARKER as u32);
    x_ret(&mut c);
    (c, handler)
}

#[test]
fn t5_sigaction_self_delivery() {
    let (ct, st) = host_cpu();
    let dv = data_va(4);
    let (code, handler) = t5_code(dv);
    // struct sigaction { handler, mask, flags } + marker word.
    let mut data = vec![0u8; 20];
    data[0..8].copy_from_slice(&handler.to_le_bytes());
    let img = build_import_guest(
        ct,
        st,
        &["_sigaction", "_getpid", "_kill", "_exit"],
        &code,
        &data,
    );
    let path = write_guest("t5", &img);
    let out = run(&path);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
}

// ---------------------------------------------------------------------------
// T6: posix_spawn /bin/true + wait (status word must be 0).
// ---------------------------------------------------------------------------

// Data layout: path "/bin/true\0" @0 (10 bytes), argv @16 ([path, 0]),
// pid @32 (u32), status @36 (u32).
fn t6_data(dv: u64) -> Vec<u8> {
    let mut d = vec![0u8; 40];
    d[0..10].copy_from_slice(b"/bin/true\0");
    d[16..24].copy_from_slice(&dv.to_le_bytes());
    d
}

#[cfg(target_arch = "aarch64")]
fn t6_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    // posix_spawn(&pid, path, NULL, NULL, argv, NULL): POSIX
    // order (pid, path, actions, attrp, argv, envp).
    let s = site(&c);
    a_adr(&mut c, 0, s, dv + 32);
    let s = site(&c);
    a_adr(&mut c, 1, s, dv);
    a_movz(&mut c, 2, 0, 0, true);
    a_movz(&mut c, 3, 0, 0, true);
    let s = site(&c);
    a_adr(&mut c, 4, s, dv + 16);
    a_movz(&mut c, 5, 0, 0, true);
    let s = site(&c);
    a_call_got(&mut c, s, slot(0));
    a_subs_x_imm(&mut c, 31, 0, 0);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 10, 2);
    b_patch(&mut c, ok);
    // waitpid(pid, &status, 0) must return the pid (> 0).
    let s = site(&c);
    a_adr(&mut c, 0, s, dv + 32);
    a_ldr_w(&mut c, 0, 0);
    a_mov_w(&mut c, 19, 0);
    a_mov_w(&mut c, 0, 19);
    let s = site(&c);
    a_adr(&mut c, 1, s, dv + 36);
    a_movz(&mut c, 2, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(1));
    a_subs_x_imm(&mut c, 31, 0, 0);
    let ok = b_ok(&mut c, 12); // gt
    epilogue(&mut c, 11, 2);
    b_patch(&mut c, ok);
    // Exited-0 children leave a zero status word.
    let s = site(&c);
    a_adr(&mut c, 0, s, dv + 36);
    a_ldr_w(&mut c, 0, 0);
    a_cmp_w_imm(&mut c, 0, 0);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 12, 2);
    b_patch(&mut c, ok);
    a_movz(&mut c, 0, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(2));
    c
}

#[cfg(target_arch = "x86_64")]
fn t6_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    // POSIX order: (pid, path, actions, attrp, argv, envp).
    let s = site(&c);
    x_lea_rip(&mut c, 7, s, dv + 32);
    let s = site(&c);
    x_lea_rip(&mut c, 6, s, dv);
    x_xor_edx(&mut c);
    x_xor_ecx(&mut c);
    let s = site(&c);
    x_lea_rip(&mut c, 8, s, dv + 16);
    x_xor_r9d(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(0));
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 10, 2);
    j_patch(&mut c, ok);
    let s = site(&c);
    x_load_edi_rip(&mut c, s, dv + 32);
    x_mov_ebx_edi(&mut c); // save pid (edi holds it, not eax)
    x_mov_edi_ebx(&mut c);
    let s = site(&c);
    x_lea_rip(&mut c, 6, s, dv + 36);
    x_xor_edx(&mut c);
    let s = site(&c);
    x_call_got(&mut c, s, slot(1));
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x8f); // jg
    epilogue(&mut c, 11, 2);
    j_patch(&mut c, ok);
    let s = site(&c);
    x_load_eax_rip(&mut c, s, dv + 36);
    x_test_eax(&mut c);
    let ok = j_ok(&mut c, 0x84);
    epilogue(&mut c, 12, 2);
    j_patch(&mut c, ok);
    x_mov_rdi_imm32(&mut c, 0);
    let s = site(&c);
    x_call_got(&mut c, s, slot(2));
    c
}

#[test]
fn t6_spawn_true_wait_status() {
    let (ct, st) = host_cpu();
    let dv = data_va(3);
    let img = build_import_guest(
        ct,
        st,
        &["_posix_spawn", "_waitpid", "_exit"],
        &t6_code(dv),
        &t6_data(dv),
    );
    let path = write_guest("t6", &img);
    let out = run(&path);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
}

// ---------------------------------------------------------------------------
// T7: ceil(0.5) == 1.0 via the libm shim.
// ---------------------------------------------------------------------------

fn t7_data() -> Vec<u8> {
    let mut d = vec![];
    d.extend_from_slice(&0x3fe0_0000_0000_0000u64.to_le_bytes()); // 0.5
    d.extend_from_slice(&0x3ff0_0000_0000_0000u64.to_le_bytes()); // 1.0
    d
}

#[cfg(target_arch = "aarch64")]
fn t7_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    let s = site(&c);
    a_adr(&mut c, 0, s, dv);
    a_ldr_d(&mut c, 0, 0);
    let s = site(&c);
    a_call_got(&mut c, s, slot(0));
    let s = site(&c);
    a_adr(&mut c, 0, s, dv + 8);
    a_ldr_d(&mut c, 1, 0);
    a_fcmp_d(&mut c, 1, 0);
    a_cset_eq(&mut c, 0);
    a_cmp_w_imm(&mut c, 0, 1);
    let ok = b_ok(&mut c, 0);
    epilogue(&mut c, 10, 1);
    b_patch(&mut c, ok);
    a_movz(&mut c, 0, 0, 0, false);
    let s = site(&c);
    a_call_got(&mut c, s, slot(1));
    c
}

#[cfg(target_arch = "x86_64")]
fn t7_code(dv: u64) -> Vec<u8> {
    let mut c = vec![];
    let s = site(&c);
    x_movsd_xmm0_rip(&mut c, s, dv);
    let s = site(&c);
    x_call_got(&mut c, s, slot(0));
    let s = site(&c);
    x_movsd_xmm1_rip(&mut c, s, dv + 8);
    x_ucomisd_01(&mut c);
    x_sete_al(&mut c);
    x_movzx_eax_al(&mut c);
    x_cmp_eax_0(&mut c);
    let ok = j_ok(&mut c, 0x85); // jne: equal (al == 1)
    epilogue(&mut c, 10, 1);
    j_patch(&mut c, ok);
    x_mov_rdi_imm32(&mut c, 0);
    let s = site(&c);
    x_call_got(&mut c, s, slot(1));
    c
}

#[test]
fn t7_ceil_half_is_one() {
    let (ct, st) = host_cpu();
    let dv = data_va(2);
    let img = build_import_guest(ct, st, &["_ceil", "_exit"], &t7_code(dv), &t7_data());
    let path = write_guest("t7", &img);
    let out = run(&path);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
}

// ---------------------------------------------------------------------------
// T8: libSystem init order is visible on stderr.
// ---------------------------------------------------------------------------

#[cfg(target_arch = "aarch64")]
fn fault_code() -> Vec<u8> {
    let mut c = vec![];
    a_movz(&mut c, 0, 0, 0, true);
    a_ldr_x(&mut c, 0, 0); // ldr x0, [x0] with x0 == 0
    c
}

#[cfg(target_arch = "x86_64")]
fn fault_code() -> Vec<u8> {
    let mut c = vec![];
    x_mov_r32_imm(&mut c, 0, 0);
    x_load_eax_rax(&mut c); // mov eax, [rax] with rax == 0
    c
}

#[test]
fn fault_catcher_reports_segv() {
    let (ct, st) = host_cpu();
    let img = build_import_guest(ct, st, &["_exit"], &fault_code(), &[]);
    let path = write_guest("fault", &img);
    let out = run(&path);
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(
        out.status.signal(),
        Some(11),
        "code {:?} stderr {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("fatal signal 11"), "stderr:\n{err}");
    assert!(err.contains("addr 0x0000000000000000"), "stderr:\n{err}");
    // PC lands inside the guest entry page (proves the ucontext
    // offset, not just the signal).
    assert!(err.contains("pc 0x0000000010001"), "stderr:\n{err}");
}

#[test]
fn t8_init_order_on_stderr() {
    let path = write_guest("t8", &t1_image());
    let out = run(&path);
    assert_eq!(
        out.status.code(),
        Some(7),
        "stderr: {} ({})",
        String::from_utf8_lossy(&out.stderr),
        status_str(&out)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    let want = "init: __libkernel_init -> __libplatform_init -> __pthread_init -> _libc_initializer -> __malloc_init -> _dyld_initializer -> libdispatch_init -> _libxpc_initializer";
    assert!(err.contains(want), "stderr:\n{err}");
}
