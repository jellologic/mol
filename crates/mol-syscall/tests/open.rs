// SPDX-License-Identifier: MIT OR Apache-2.0
//! `open` conformance (P1.2.V1): create/truncate roundtrip, errno
//! names, locks, limited fds, rejected flags.
#![cfg(target_os = "linux")]

mod common;

use core::ffi::c_char;
use mol_syscall::errno::*;
use mol_syscall::flags::*;

#[test]
fn create_write_read_close_roundtrip() {
    let path = common::tmp("open-rw");
    let c = common::cstr(&path);
    let fd = mol_syscall::mol_open(c.as_ptr(), O_WRONLY | O_CREAT | O_TRUNC, 0o644);
    assert!(
        fd >= 0,
        "errno {:?}",
        mol_syscall::errno::name(mol_syscall::errcell::get())
    );
    let msg = b"mol-smoke";
    let n = mol_syscall::mol_write(fd, msg.as_ptr() as *const c_char, msg.len());
    assert_eq!(n, msg.len() as isize);
    assert_eq!(mol_syscall::mol_close(fd), 0);

    let fd = mol_syscall::mol_open(c.as_ptr(), O_RDONLY, 0);
    assert!(fd >= 0);
    let mut buf = [0u8; 16];
    let n = mol_syscall::mol_read(fd, buf.as_mut_ptr(), buf.len());
    assert_eq!(n, msg.len() as isize);
    assert_eq!(&buf[..msg.len()], msg);
    // EOF reads 0.
    let n = mol_syscall::mol_read(fd, buf.as_mut_ptr(), buf.len());
    assert_eq!(n, 0);
    assert_eq!(mol_syscall::mol_close(fd), 0);
    std::fs::remove_file(&path).ok();
}

#[test]
fn errno_names_mac_correct() {
    // Missing file: ENOENT = 2 on both, name must be ENOENT.
    let missing = common::cstr(&common::tmp("open-missing-xyz"));
    assert_eq!(mol_syscall::mol_open(missing.as_ptr(), O_RDONLY, 0), -1);
    common::assert_errno(ENOENT);
    assert_eq!(name(mol_syscall::errcell::get()), Some("ENOENT"));

    // O_CREAT|O_EXCL on an existing file: EEXIST.
    let path = common::tmp("open-excl");
    let c = common::cstr(&path);
    let fd = mol_syscall::mol_open(c.as_ptr(), O_WRONLY | O_CREAT, 0o644);
    assert!(fd >= 0);
    assert_eq!(mol_syscall::mol_close(fd), 0);
    assert_eq!(
        mol_syscall::mol_open(c.as_ptr(), O_WRONLY | O_CREAT | O_EXCL, 0o644),
        -1
    );
    common::assert_errno(EEXIST);

    // Bad access mode: EINVAL.
    assert_eq!(mol_syscall::mol_open(c.as_ptr(), 3, 0), -1);
    common::assert_errno(EINVAL);
    std::fs::remove_file(&path).ok();
}

#[test]
fn bad_fd_is_ebadf() {
    let mut buf = [0u8; 4];
    assert_eq!(mol_syscall::mol_read(-1, buf.as_mut_ptr(), 4), -1);
    common::assert_errno(EBADF);
    assert_eq!(mol_syscall::mol_write(-1, buf.as_ptr() as *const c_char, 4), -1);
    common::assert_errno(EBADF);
    assert_eq!(mol_syscall::mol_close(-1), -1);
    common::assert_errno(EBADF);
}

#[test]
fn locks_and_limited_fds() {
    let path = common::tmp("open-lock");
    let c = common::cstr(&path);
    let fd = mol_syscall::mol_open(c.as_ptr(), O_WRONLY | O_CREAT, 0o644);
    assert!(fd >= 0);
    assert_eq!(mol_syscall::mol_close(fd), 0);
    // SHLOCK/EXLOCK open.
    let fd = mol_syscall::mol_open(c.as_ptr(), O_RDONLY | O_SHLOCK, 0);
    assert!(
        fd >= 0,
        "shlock errno {:?}",
        mol_syscall::errno::name(mol_syscall::errcell::get())
    );
    assert_eq!(mol_syscall::mol_close(fd), 0);
    let fd = mol_syscall::mol_open(c.as_ptr(), O_RDONLY | O_EXLOCK, 0);
    assert!(
        fd >= 0,
        "exlock errno {:?}",
        mol_syscall::errno::name(mol_syscall::errcell::get())
    );
    assert_eq!(mol_syscall::mol_close(fd), 0);
    // EVTONLY opens (O_PATH cousin); I/O on it fails.
    let fd = mol_syscall::mol_open(c.as_ptr(), O_EVTONLY, 0);
    assert!(
        fd >= 0,
        "evt errno {:?}",
        mol_syscall::errno::name(mol_syscall::errcell::get())
    );
    let mut buf = [0u8; 4];
    assert_eq!(mol_syscall::mol_read(fd, buf.as_mut_ptr(), 4), -1);
    assert_eq!(mol_syscall::mol_close(fd), 0);
    std::fs::remove_file(&path).ok();
}

#[test]
fn exotic_flags_rejected() {
    let path = common::tmp("open-exotic");
    let c = common::cstr(&path);
    for flag in [O_ASYNC, O_CLOFORK, O_UNIQUE, O_NOFOLLOW_ANY, O_POPUP] {
        assert_eq!(
            mol_syscall::mol_open(c.as_ptr(), O_RDONLY | flag, 0),
            -1,
            "flag {flag:#x}"
        );
        common::assert_errno(EINVAL);
    }
    std::fs::remove_file(&path).ok();
}
