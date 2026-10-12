// SPDX-License-Identifier: MIT OR Apache-2.0
//! `open` / `read` / `write` / `close` handlers.
//!
//! Paths are bounded pre-scans (`PATH_MAX` 1024, the Darwin limit):
//! an unterminated guest string fails `ENAMETOOLONG` instead of
//! over-reading. Wild pointers past the mapped prefix remain a
//! crash-divergence vs `EFAULT` (doc 43 OQ).

#[cfg(target_os = "linux")]
use crate::errcell;
#[cfg(target_os = "linux")]
use crate::errno::{from_linux, ENAMETOOLONG};
#[cfg(target_os = "linux")]
use crate::flags::{translate_open, OpenLock};
#[cfg(target_os = "linux")]
use core::ffi::c_char;

/// Darwin `PATH_MAX` (1024).
pub const PATH_MAX: usize = 1024;

/// Borrow a guest C string with a bounded scan. Returns the byte
/// slice plus its address for the raw call.
#[cfg(target_os = "linux")]
fn guest_str(ptr: *const c_char) -> Result<(*const u8, usize), i32> {
    if ptr.is_null() {
        return Err(crate::errno::EFAULT);
    }
    // SAFETY: bounded scan of at most PATH_MAX+1 bytes; a wild
    // pointer can still fault (documented divergence), but a merely
    // unterminated one cannot over-read past the bound.
    let ptr = ptr as *const u8;
    unsafe {
        let mut len = 0usize;
        while len <= PATH_MAX {
            if *ptr.add(len) == 0 {
                break;
            }
            len += 1;
        }
        if len > PATH_MAX {
            return Err(ENAMETOOLONG);
        }
        Ok((ptr, len))
    }
}

#[cfg(target_os = "linux")]
fn fail<T>(no: i32) -> Result<T, i32> {
    errcell::set(no);
    Err(no)
}

/// `open(path, flags[, mode])`.
#[cfg(target_os = "linux")]
pub fn open(path: *const c_char, flags: i32, mode: u32) -> Result<i32, i32> {
    use crate::raw;
    let of = match translate_open(flags) {
        Ok(f) => f,
        Err(no) => return fail(no),
    };
    let (p, _) = match guest_str(path) {
        Ok(s) => s,
        Err(no) => return fail(no),
    };
    let fd = match raw::openat(raw::AT_FDCWD, p as u64, of.linux as u64, mode as u64) {
        Ok(fd) => fd as i32,
        Err(linux) => return fail(from_linux(linux)),
    };
    if let Some(lock) = of.lock {
        let op = match lock {
            OpenLock::Shared => raw::LOCK_SH,
            OpenLock::Exclusive => raw::LOCK_EX,
        };
        if let Err(linux) = raw::flock(fd as i64, op) {
            let no = from_linux(linux);
            let _ = raw::close(fd as i64);
            return fail(no);
        }
    }
    Ok(fd)
}

/// `read(fd, buf, n)`: single attempt, no userspace restart.
#[cfg(target_os = "linux")]
pub fn read(fd: i32, buf: *mut u8, len: usize) -> Result<usize, i32> {
    if buf.is_null() && len > 0 {
        return fail(crate::errno::EFAULT);
    }
    match crate::raw::read(fd as i64, buf as u64, len as u64) {
        Ok(n) => Ok(n as usize),
        Err(linux) => fail(from_linux(linux)),
    }
}

/// `write(fd, buf, n)`: single attempt (short writes reported, not
/// retried — matches the wrapper, which returns what the kernel did).
#[cfg(target_os = "linux")]
pub fn write(fd: i32, buf: *const c_char, len: usize) -> Result<usize, i32> {
    if buf.is_null() && len > 0 {
        return fail(crate::errno::EFAULT);
    }
    match crate::raw::write(fd as i64, buf as u64, len as u64) {
        Ok(n) => Ok(n as usize),
        Err(linux) => fail(from_linux(linux)),
    }
}

/// `close(fd)`.
#[cfg(target_os = "linux")]
pub fn close(fd: i32) -> Result<(), i32> {
    match crate::raw::close(fd as i64) {
        Ok(_) => Ok(()),
        Err(linux) => fail(from_linux(linux)),
    }
}
