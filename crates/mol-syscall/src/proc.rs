// SPDX-License-Identifier: MIT OR Apache-2.0
//! `exit` / `_exit` / `getpid` / `kill` / `waitpid` handlers.
//!
//! `exit` here is the `_exit` path (no atexit/stdio flush: stdio
//! arrives with libc in P1.3; G1's hello gate uses direct syscalls).
//! Wait-status encoding and `WNOHANG`/`WUNTRACED` values match on
//! both OSes (BSD heritage), so `waitpid` passes status through.

#[cfg(target_os = "linux")]
use crate::errcell;
#[cfg(target_os = "linux")]
use crate::errno::{from_linux, EINVAL};

pub const WNOHANG: i32 = 1;
pub const WUNTRACED: i32 = 2;

/// `getpid()`.
#[cfg(target_os = "linux")]
pub fn getpid() -> i32 {
    crate::raw::getpid() as i32
}

/// `kill(pid, signo)` with Darwin signal numbers.
#[cfg(target_os = "linux")]
pub fn kill(pid: i32, signo: i32) -> Result<(), i32> {
    // Signo 0 is the existence probe on both OSes.
    if signo == 0 {
        return match crate::raw::kill(pid as i64, 0) {
            Ok(_) => Ok(()),
            Err(linux) => {
                let no = from_linux(linux);
                errcell::set(no);
                Err(no)
            }
        };
    }
    if signo < 0 || signo >= crate::signal::NSIG_DARWIN {
        errcell::set(EINVAL);
        return Err(EINVAL);
    }
    let sig = match crate::signal::signo_to_linux(signo) {
        Ok(s) => s,
        Err(no) => {
            errcell::set(no);
            return Err(no);
        }
    };
    match crate::raw::kill(pid as i64, sig as u64) {
        Ok(_) => Ok(()),
        Err(linux_err) => {
            let no = from_linux(linux_err);
            errcell::set(no);
            Err(no)
        }
    }
}

/// `waitpid(pid, status, options)`. `ECHILD` when no children match
/// (Linux reports it; translated 1:1).
#[cfg(target_os = "linux")]
pub fn waitpid(pid: i32, status: *mut i32, options: i32) -> Result<i32, i32> {
    if options & !(WNOHANG | WUNTRACED) != 0 {
        errcell::set(EINVAL);
        return Err(EINVAL);
    }
    if status.is_null() {
        // waitpid(NULL status) is legal; wait4 with a null out-param.
        return match crate::raw::wait4(pid as i64, 0, options as u64) {
            Ok(got) => Ok(got as i32),
            Err(linux) => {
                let no = from_linux(linux);
                errcell::set(no);
                Err(no)
            }
        };
    }
    let mut st: i32 = 0;
    match crate::raw::wait4(pid as i64, &mut st as *mut i32 as u64, options as u64) {
        Ok(got) => {
            // SAFETY: guest out-pointer for one int; wild pointers
            // fault (documented divergence, doc 43 OQ).
            unsafe {
                *status = st;
            }
            Ok(got as i32)
        }
        Err(linux) => {
            let no = from_linux(linux);
            errcell::set(no);
            Err(no)
        }
    }
}

/// `exit(status)` / `_exit(status)`: diverges via `exit_group`.
/// atexit handlers and stdio flush arrive with libc (P1.3).
#[cfg(target_os = "linux")]
pub fn exit(status: i32) -> ! {
    crate::raw::exit_group(status & 0xff)
}
