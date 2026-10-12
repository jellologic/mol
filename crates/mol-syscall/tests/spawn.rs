// SPDX-License-Identifier: MIT OR Apache-2.0
//! `posix_spawn`/`waitpid` conformance (P1.2.V1): exit-status
//! plumbing, spawn errors as return values, Mach-O dispatch.
#![cfg(target_os = "linux")]

mod common;

use core::ffi::c_char;
use mol_syscall::errno::*;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;

fn cstr(s: &str) -> CString {
    CString::new(s).unwrap()
}

fn spawn_wait(file: &str, args: &[&str]) -> (i32, i32) {
    let f = cstr(file);
    let argv: Vec<CString> = args.iter().map(|a| cstr(a)).collect();
    let ptrs: Vec<*const c_char> = argv.iter().map(|a| a.as_ptr() as *const c_char).collect();
    let mut argvp = ptrs.clone();
    argvp.push(core::ptr::null());
    let mut pid: i32 = -1;
    let rc = mol_syscall::mol_posix_spawn(
        &mut pid,
        f.as_ptr() as *const c_char,
        argvp.as_ptr(),
        core::ptr::null(),
        core::ptr::null(),
        core::ptr::null(),
    );
    assert_eq!(rc, 0, "spawn {file} failed: {:?}", name(rc));
    assert!(pid > 0);
    let mut status: i32 = 0;
    let got = mol_syscall::mol_waitpid(pid, &mut status, 0);
    assert_eq!(got, pid);
    (pid, status)
}

fn exit_status(status: i32) -> i32 {
    (status >> 8) & 0xff
}

#[test]
fn true_false_exit_status() {
    let (_, st) = spawn_wait("/bin/true", &["true"]);
    assert_eq!(exit_status(st), 0);
    let (_, st) = spawn_wait("/bin/false", &["false"]);
    assert_eq!(exit_status(st), 1);
}

#[test]
fn shell_exit_code_flows() {
    let (_, st) = spawn_wait("/bin/sh", &["sh", "-c", "exit 41"]);
    assert_eq!(exit_status(st), 41);
}

#[test]
fn spawn_errors_return_errno_not_cell() {
    // Missing target: ENOENT as the RETURN value; the errno cell is
    // untouched (POSIX: posix_spawn never sets errno).
    let f = cstr("/nonexistent-mol-spawn-target");
    let a0 = cstr("x");
    let argvp = [a0.as_ptr() as *const c_char, core::ptr::null()];
    mol_syscall::errcell::set(0);
    let mut pid: i32 = -1;
    let rc = mol_syscall::mol_posix_spawn(
        &mut pid,
        f.as_ptr() as *const c_char,
        argvp.as_ptr(),
        core::ptr::null(),
        core::ptr::null(),
        core::ptr::null(),
    );
    assert_eq!(rc, ENOENT, "name {:?}", name(rc));
    assert_eq!(mol_syscall::errcell::get(), 0);
}

#[test]
fn spawn_rejects_non_smoke_args() {
    let f = cstr("/bin/true");
    let a0 = cstr("true");
    let argvp = [a0.as_ptr() as *const c_char, core::ptr::null()];
    let dummy = cstr("dummy");
    let mut pid: i32 = -1;
    // Non-NULL file_actions.
    assert_eq!(
        mol_syscall::mol_posix_spawn(
            &mut pid,
            f.as_ptr() as *const c_char,
            argvp.as_ptr(),
            core::ptr::null(),
            dummy.as_ptr() as *const u8,
            core::ptr::null(),
        ),
        EINVAL
    );
    // Non-NULL attrp.
    assert_eq!(
        mol_syscall::mol_posix_spawn(
            &mut pid,
            f.as_ptr() as *const c_char,
            argvp.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            dummy.as_ptr() as *const u8,
        ),
        EINVAL
    );
    // NULL argv.
    assert_eq!(
        mol_syscall::mol_posix_spawn(
            &mut pid,
            f.as_ptr() as *const c_char,
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null(),
        ),
        EINVAL
    );
}

#[test]
fn waitpid_wnohang_and_echild() {
    // Reap a child, then wait on it again → ECHILD. (Never
    // waitpid(-1) here: sibling tests' children share this process,
    // and stealing them flakes both sides.)
    let f = cstr("/bin/true");
    let a0 = cstr("true");
    let argvp = [a0.as_ptr() as *const c_char, core::ptr::null()];
    let mut pid: i32 = -1;
    assert_eq!(
        mol_syscall::mol_posix_spawn(
            &mut pid,
            f.as_ptr() as *const c_char,
            argvp.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null(),
        ),
        0
    );
    let mut status: i32 = 0;
    assert_eq!(mol_syscall::mol_waitpid(pid, &mut status, 0), pid);
    assert_eq!(mol_syscall::mol_waitpid(pid, &mut status, 0), -1);
    common::assert_errno(ECHILD);
    // Bad options (checked before pid lookup).
    assert_eq!(mol_syscall::mol_waitpid(pid, &mut status, 0x4000), -1);
    common::assert_errno(EINVAL);
    // WNOHANG on a live child returns 0, then reaps.
    let f = cstr("/bin/sh");
    let a0 = cstr("sh");
    let a1 = cstr("-c");
    let a2 = cstr("sleep 0.4");
    let argvp = [
        a0.as_ptr() as *const c_char,
        a1.as_ptr() as *const c_char,
        a2.as_ptr() as *const c_char,
        core::ptr::null(),
    ];
    let mut pid: i32 = -1;
    assert_eq!(
        mol_syscall::mol_posix_spawn(
            &mut pid,
            f.as_ptr() as *const c_char,
            argvp.as_ptr(),
            core::ptr::null(),
            core::ptr::null(),
            core::ptr::null(),
        ),
        0
    );
    assert_eq!(mol_syscall::mol_waitpid(pid, &mut status, 1 /*WNOHANG*/), 0);
    assert_eq!(mol_syscall::mol_waitpid(pid, &mut status, 0), pid);
    assert_eq!(exit_status(status), 0);
}

#[test]
fn macho_target_dispatches_to_loader_override() {
    // A Mach-O-magic file routes through the loader path; the
    // MOL_SPAWN_LOADER override (test-only) names the executable so
    // the test stays hermetic. The override script records argv.
    let dir = common::tmp("spawn-macho");
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("fake-macho");
    std::fs::write(&target, [0xcf, 0xfa, 0xed, 0xfe, 0, 0, 0, 0]).unwrap();
    let record = dir.join("argv.txt");
    let script = dir.join("loader.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nexit 7\n",
            record.display()
        ),
    )
    .unwrap();
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let f = CString::new(target.as_os_str().as_bytes()).unwrap();
    let a0 = CString::new(target.as_os_str().as_bytes()).unwrap();
    let a1 = cstr("hello-arg");
    let argvp = [
        a0.as_ptr() as *const c_char,
        a1.as_ptr() as *const c_char,
        core::ptr::null(),
    ];
    std::env::set_var("MOL_SPAWN_LOADER", &script);
    let mut pid: i32 = -1;
    let rc = mol_syscall::mol_posix_spawn(
        &mut pid,
        f.as_ptr() as *const c_char,
        argvp.as_ptr(),
        core::ptr::null(),
        core::ptr::null(),
        core::ptr::null(),
    );
    std::env::remove_var("MOL_SPAWN_LOADER");
    assert_eq!(rc, 0, "spawn failed: {:?}", name(rc));
    let mut status: i32 = 0;
    assert_eq!(mol_syscall::mol_waitpid(pid, &mut status, 0), pid);
    assert_eq!(exit_status(status), 7);
    // The override is a script, so the kernel replaces our argv[0]
    // ("mol-loader") with the interpreter+script path; "$@" observes
    // our argv[1..]. (The real loader is an ELF: no replacement, full
    // argv visible.)
    let lines = std::fs::read_to_string(&record).unwrap();
    let lines: Vec<&str> = lines.lines().collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0], "run");
    assert_eq!(lines[1], target.display().to_string());
    assert_eq!(lines[2], "hello-arg");
    std::fs::remove_dir_all(&dir).ok();
}
