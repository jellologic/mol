// SPDX-License-Identifier: MIT OR Apache-2.0
//! BSD syscall number translation + Darwin-only shims (doc 04).
//!
//! Contract: wrappers, not traps, are the compat boundary (doc 11
//! F1). Same errno names, same wrapper-observable side effects.
//! Raw-trap behavior is diagnostic signal only, never the gate.
//!
//! Same-ISA in-process execution means guests call these handlers
//! directly with the C ABI: the guest's registers are already the
//! handler's arguments. The shim dylib (`mol-libsystem`) binds each
//! export to a trampoline slot the loader fills with
//! [`export_addr`].

pub mod atfork;
pub mod chk;
pub mod errcell;
pub mod errno;
pub mod flags;
pub mod io;
pub mod mmap;
pub mod proc;
pub mod signal;
#[cfg(target_os = "linux")]
pub mod spawn;

#[cfg(target_os = "linux")]
mod raw;

#[cfg(target_os = "linux")]
use core::ffi::{c_char, c_int, c_uint};

/// Smoke-subset exports in slot order (shared with the shim
/// generator; order is ABI). `_open`/`_read`/`_write`/`_mmap`/
/// `_sigaction`/`_posix_spawn` are the gated P1.2.2 set; the rest are
/// documented bonus exports the E2E proof needs (`_close` for fd
/// hygiene, `_waitpid` to observe spawn, `_exit`/`__exit` for return
/// from `main`, `_kill` to deliver test signals, `___error` for the
/// errno cell, stack-protector pair for clang-built guests).
pub const EXPORT_NAMES: &[&str] = &[
    "_open",
    "_read",
    "_write",
    "_close",
    "_mmap",
    "_munmap",
    "_mprotect",
    "_sigaction",
    "_posix_spawn",
    "_waitpid",
    "_exit",
    "__exit",
    "___error",
    "_getpid",
    "_kill",
    "___stack_chk_guard",
    "___stack_chk_fail",
];

/// Handler address (or data value, for `___stack_chk_guard`) for a
/// shim export. The loader calls [`chk::init_guard`] before filling
/// slots so the guard value is live.
#[cfg(target_os = "linux")]
pub fn export_addr(name: &str) -> Option<u64> {
    Some(match name {
        "_open" => mol_open as *const () as u64,
        "_read" => mol_read as *const () as u64,
        "_write" => mol_write as *const () as u64,
        "_close" => mol_close as *const () as u64,
        "_mmap" => mol_mmap as *const () as u64,
        "_munmap" => mol_munmap as *const () as u64,
        "_mprotect" => mol_mprotect as *const () as u64,
        "_sigaction" => mol_sigaction as *const () as u64,
        "_posix_spawn" => mol_posix_spawn as *const () as u64,
        "_waitpid" => mol_waitpid as *const () as u64,
        "_exit" | "__exit" => mol_exit as *const () as u64,
        "___error" => mol___error as *const () as u64,
        "_getpid" => mol_getpid as *const () as u64,
        "_kill" => mol_kill as *const () as u64,
        "___stack_chk_guard" => chk::guard(),
        "___stack_chk_fail" => mol___stack_chk_fail as *const () as u64,
        _ => return None,
    })
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_open(path: *const c_char, flags: c_int, mode: c_uint) -> c_int {
    match io::open(path, flags, mode) {
        Ok(fd) => fd,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_read(fd: c_int, buf: *mut u8, len: usize) -> isize {
    match io::read(fd, buf, len) {
        Ok(n) => n as isize,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_write(fd: c_int, buf: *const c_char, len: usize) -> isize {
    match io::write(fd, buf, len) {
        Ok(n) => n as isize,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_close(fd: c_int) -> c_int {
    match io::close(fd) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_mmap(
    addr: *mut u8,
    len: usize,
    prot: c_int,
    flags: c_int,
    fd: c_int,
    off: i64,
) -> *mut u8 {
    match mmap::mmap(addr, len, prot, flags, fd, off) {
        Ok(at) => at,
        Err(_) => -1isize as *mut u8, // MAP_FAILED
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_munmap(addr: *mut u8, len: usize) -> c_int {
    match mmap::munmap(addr, len) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_mprotect(addr: *mut u8, len: usize, prot: c_int) -> c_int {
    match mmap::mprotect(addr, len, prot) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_sigaction(
    signo: c_int,
    act: *const signal::DarwinSigaction,
    oldact: *mut signal::DarwinSigaction,
) -> c_int {
    // SAFETY: guest pointers; null means absent, wild faults
    // (documented divergence, doc 43 OQ).
    let act = unsafe { act.as_ref() };
    let oldact = unsafe { oldact.as_mut() };
    match signal::sigaction(signo, act, oldact) {
        Ok(()) => 0,
        Err(no) => {
            errcell::set(no);
            -1
        }
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_posix_spawn(
    pid: *mut i32,
    path: *const c_char,
    file_actions: *const u8,
    attrp: *const u8,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    // POSIX: returns the errno number directly, never sets errno.
    // Arg order is the Darwin/POSIX one: (pid, path, actions,
    // attrp, argv, envp).
    match spawn::posix_spawn(pid, path, file_actions, attrp, argv, envp) {
        Ok(_) => 0,
        Err(no) => no,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int {
    match proc::waitpid(pid, status, options) {
        Ok(got) => got,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_exit(status: c_int) -> ! {
    proc::exit(status)
}

#[cfg(target_os = "linux")]
#[allow(non_snake_case)] // mirrors the ___error symbol deliberately
pub extern "C" fn mol___error() -> *mut c_int {
    errcell::error_ptr()
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_getpid() -> c_int {
    proc::getpid()
}

#[cfg(target_os = "linux")]
pub extern "C" fn mol_kill(pid: c_int, signo: c_int) -> c_int {
    match proc::kill(pid, signo) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

#[cfg(target_os = "linux")]
#[allow(non_snake_case)] // mirrors the ___stack_chk_fail symbol deliberately
pub extern "C" fn mol___stack_chk_fail() -> ! {
    chk::stack_chk_fail()
}
