// SPDX-License-Identifier: MIT OR Apache-2.0
//! `getpid`/`kill`/`___error`/`___stack_chk_guard` + export-registry
//! conformance (P1.2.V1). The `___stack_chk_fail` path is exercised
//! in a subprocess (it diverges with `SIGABRT`).
#![cfg(target_os = "linux")]

use core::ffi::c_char;

#[test]
fn getpid_matches_std() {
    assert_eq!(mol_syscall::mol_getpid(), std::process::id() as i32);
}

#[test]
fn error_cell_ptr_roundtrip() {
    unsafe {
        mol_syscall::errcell::set(12);
        let p = mol_syscall::mol___error();
        assert!(!p.is_null());
        assert_eq!(*p, 12);
        *p = 0;
        assert_eq!(mol_syscall::errcell::get(), 0);
    }
}

#[test]
fn every_export_resolves() {
    for name in mol_syscall::EXPORT_NAMES {
        assert!(
            mol_syscall::export_addr(name).is_some(),
            "export {name} unresolved"
        );
    }
    assert_eq!(mol_syscall::export_addr("_nope"), None);
    // Distinct code addresses (the guard datum excepted).
    let mut addrs: Vec<u64> = mol_syscall::EXPORT_NAMES
        .iter()
        .filter(|n| **n != "___stack_chk_guard")
        .map(|n| mol_syscall::export_addr(n).unwrap())
        .collect();
    addrs.sort();
    addrs.dedup();
    // _exit and __exit share one address by design.
    assert_eq!(addrs.len(), mol_syscall::EXPORT_NAMES.len() - 2);
}

#[test]
fn guard_value_lifecycle() {
    // init_guard produces a stable nonzero value.
    let g = mol_syscall::chk::init_guard();
    assert_ne!(g, 0);
    assert_eq!(mol_syscall::chk::guard(), g);
    assert_eq!(mol_syscall::export_addr("___stack_chk_guard"), Some(g));
}

#[test]
fn chk_fail_child() {
    // Subprocess body for chk_fail_dies_sigabrt (selected by env).
    if std::env::var("MOL_CHK_CHILD").is_err() {
        return;
    }
    mol_syscall::mol___stack_chk_fail()
}

#[test]
fn chk_fail_dies_sigabrt() {
    // Spawn this test binary filtered to the child test above.
    let me = std::env::current_exe().unwrap();
    let me_s = me.to_str().unwrap().to_string();
    let a0 = std::ffi::CString::new(me_s.clone()).unwrap();
    let a1 = std::ffi::CString::new("--exact").unwrap();
    let a2 = std::ffi::CString::new("chk_fail_child").unwrap();
    let a3 = std::ffi::CString::new("--nocapture").unwrap();
    let argvp = [
        a0.as_ptr() as *const c_char,
        a1.as_ptr() as *const c_char,
        a2.as_ptr() as *const c_char,
        a3.as_ptr() as *const c_char,
        core::ptr::null(),
    ];
    // Minimal env with the selector (inherit would also work; this is
    // explicit).
    let e0 = std::ffi::CString::new("MOL_CHK_CHILD=1").unwrap();
    let path = std::ffi::CString::new(format!(
        "PATH={}",
        std::env::var("PATH").unwrap_or_default()
    ))
    .unwrap();
    let envp = [
        e0.as_ptr() as *const c_char,
        path.as_ptr() as *const c_char,
        core::ptr::null(),
    ];
    let mut pid: i32 = -1;
    let rc = mol_syscall::mol_posix_spawn(
        &mut pid,
        a0.as_ptr() as *const c_char,
        core::ptr::null(),
        core::ptr::null(),
        argvp.as_ptr(),
        envp.as_ptr(),
    );
    assert_eq!(rc, 0);
    let mut status: i32 = 0;
    assert_eq!(mol_syscall::mol_waitpid(pid, &mut status, 0), pid);
    // Signaled (not exited) with SIGABRT = 6. (The core-dump
    // bit is environment-dependent — Ubuntu defaults ulimit -c 0
    // — so it is not asserted.)
    assert_eq!(status & 0x7f, 6, "status {status:#x}");
}
