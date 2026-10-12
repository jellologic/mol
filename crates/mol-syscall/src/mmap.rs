// SPDX-License-Identifier: MIT OR Apache-2.0
//! `mmap` / `munmap` / `mprotect` handlers.

#[cfg(target_os = "linux")]
use crate::errcell;
#[cfg(target_os = "linux")]
use crate::errno::{from_linux, EINVAL};
#[cfg(target_os = "linux")]
use crate::flags::{check_prot, translate_mmap};

/// `mmap(addr, len, prot, flags, fd, off)`. Returns the mapped
/// address, or `MAP_FAILED` (`-1`) with errno set.
#[cfg(target_os = "linux")]
pub fn mmap(
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    fd: i32,
    off: i64,
) -> Result<*mut u8, i32> {
    let prot = match check_prot(prot) {
        Ok(p) => p,
        Err(no) => {
            errcell::set(no);
            return Err(no);
        }
    };
    let mf = match translate_mmap(flags) {
        Ok(f) => f,
        Err(no) => {
            errcell::set(no);
            return Err(no);
        }
    };
    if len == 0 {
        errcell::set(EINVAL);
        return Err(EINVAL);
    }
    match crate::raw::mmap(
        addr as u64,
        len as u64,
        prot as u64,
        mf.linux as u64,
        fd as i64,
        off as u64,
    ) {
        Ok(at) => Ok(at as *mut u8),
        Err(linux) => {
            let no = from_linux(linux);
            errcell::set(no);
            Err(no)
        }
    }
}

/// `munmap(addr, len)`.
#[cfg(target_os = "linux")]
pub fn munmap(addr: *mut u8, len: usize) -> Result<(), i32> {
    match crate::raw::munmap(addr as u64, len as u64) {
        Ok(_) => Ok(()),
        Err(linux) => {
            let no = from_linux(linux);
            errcell::set(no);
            Err(no)
        }
    }
}

/// `mprotect(addr, len, prot)`.
#[cfg(target_os = "linux")]
pub fn mprotect(addr: *mut u8, len: usize, prot: i32) -> Result<(), i32> {
    let prot = match check_prot(prot) {
        Ok(p) => p,
        Err(no) => {
            errcell::set(no);
            return Err(no);
        }
    };
    match crate::raw::mprotect(addr as u64, len as u64, prot as u64) {
        Ok(_) => Ok(()),
        Err(linux) => {
            let no = from_linux(linux);
            errcell::set(no);
            Err(no)
        }
    }
}
