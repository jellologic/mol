// SPDX-License-Identifier: MIT OR Apache-2.0
//! `mmap`/`munmap`/`mprotect` conformance (P1.2.V1).
#![cfg(target_os = "linux")]

mod common;

use mol_syscall::errno::*;
use mol_syscall::flags::*;

const MAP_FAILED: *mut u8 = -1isize as *mut u8;

#[test]
fn anon_map_write_protect_unmap() {
    let at = mol_syscall::mol_mmap(
        core::ptr::null_mut(),
        0x2000,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANON,
        -1,
        0,
    );
    assert_ne!(at, MAP_FAILED, "errno {:?}", mol_syscall::errcell::get());
    // Pattern roundtrip.
    unsafe {
        for i in 0..0x2000usize {
            *at.add(i) = (i & 0xff) as u8;
        }
        for i in 0..0x2000usize {
            assert_eq!(*at.add(i), (i & 0xff) as u8);
        }
    }
    // Drop to read-only, read still works.
    assert_eq!(mol_syscall::mol_mprotect(at, 0x2000, PROT_READ), 0);
    assert_eq!(unsafe { *at.add(0x123) }, 0x23);
    // Back to RW, then unmap.
    assert_eq!(
        mol_syscall::mol_mprotect(at, 0x2000, PROT_READ | PROT_WRITE),
        0
    );
    assert_eq!(mol_syscall::mol_munmap(at, 0x2000), 0);
}

#[test]
fn file_backed_mapping() {
    let path = common::tmp("mmap-file");
    std::fs::write(&path, b"0123456789abcdef").unwrap();
    let c = common::cstr(&path);
    let fd = mol_syscall::mol_open(c.as_ptr(), O_RDONLY, 0);
    assert!(fd >= 0);
    let at = mol_syscall::mol_mmap(core::ptr::null_mut(), 0x1000, PROT_READ, MAP_PRIVATE, fd, 0);
    assert_ne!(at, MAP_FAILED);
    unsafe {
        assert_eq!(*at.add(0), b'0');
        assert_eq!(*at.add(15), b'f');
    }
    assert_eq!(mol_syscall::mol_munmap(at, 0x1000), 0);
    assert_eq!(mol_syscall::mol_close(fd), 0);
    std::fs::remove_file(&path).ok();
}

#[test]
fn fixed_over_reservation() {
    // Reserve, then MAP_FIXED over the same range (safe: ours).
    let at = mol_syscall::mol_mmap(
        core::ptr::null_mut(),
        0x1000,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANON,
        -1,
        0,
    );
    assert_ne!(at, MAP_FAILED);
    let at2 = mol_syscall::mol_mmap(
        at,
        0x1000,
        PROT_READ,
        MAP_PRIVATE | MAP_ANON | MAP_FIXED,
        -1,
        0,
    );
    assert_eq!(at2, at);
    assert_eq!(unsafe { *at2 }, 0); // fresh zero page
    assert_eq!(mol_syscall::mol_munmap(at, 0x1000), 0);
}

#[test]
fn invalid_args_fail_mac_errno() {
    // Zero length.
    assert_eq!(
        mol_syscall::mol_mmap(
            core::ptr::null_mut(),
            0,
            PROT_READ,
            MAP_PRIVATE | MAP_ANON,
            -1,
            0
        ),
        MAP_FAILED
    );
    common::assert_errno(EINVAL);
    // No type bits.
    assert_eq!(
        mol_syscall::mol_mmap(core::ptr::null_mut(), 0x1000, PROT_READ, MAP_ANON, -1, 0),
        MAP_FAILED
    );
    common::assert_errno(EINVAL);
    // Bad prot bits.
    assert_eq!(
        mol_syscall::mol_mmap(
            core::ptr::null_mut(),
            0x1000,
            0x8,
            MAP_PRIVATE | MAP_ANON,
            -1,
            0
        ),
        MAP_FAILED
    );
    common::assert_errno(EINVAL);
    // mprotect bad prot.
    assert_eq!(
        mol_syscall::mol_mprotect(core::ptr::null_mut(), 0x1000, 0x10),
        -1
    );
    common::assert_errno(EINVAL);
    // munmap of unmapped memory succeeds on both OSes
    // (oracle-verified on macOS: rc=0).
    assert_eq!(mol_syscall::mol_munmap(0x100000 as *mut u8, 0x1000), 0);
}
