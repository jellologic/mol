// SPDX-License-Identifier: MIT OR Apache-2.0
//! Stack-protector ABI: `___stack_chk_guard` + `___stack_chk_fail`.
//!
//! Every clang-built binary imports both. The guard is a per-process
//! random nonzero value (like Apple's); fail writes to stderr,
//! raises `SIGABRT`, and falls back to `exit_group(134)`.

use core::sync::atomic::{AtomicU64, Ordering};

static GUARD: AtomicU64 = AtomicU64::new(0);

/// The guard value, generated once per process. Zero until
/// [`init_guard`] runs (the loader calls it at slot-fill time).
pub fn guard() -> u64 {
    GUARD.load(Ordering::Relaxed)
}

/// Generate the guard from `getrandom` (nonzero). Linux-only; other
/// hosts keep zero (tests set it explicitly).
#[cfg(target_os = "linux")]
pub fn init_guard() -> u64 {
    let mut buf = [0u8; 8];
    let v = match crate::raw::getrandom(&mut buf) {
        Ok(()) => {
            let v = u64::from_ne_bytes(buf);
            if v == 0 {
                0x1e7c_5f2d_9a3b_6d41
            } else {
                v
            }
        }
        Err(_) => 0x1e7c_5f2d_9a3b_6d41, // getrandom failed: fixed canary
    };
    GUARD.store(v, Ordering::Relaxed);
    v
}

/// `___stack_chk_fail`: stack smashing detected. Diverges with
/// `SIGABRT` (glibc `abort` parity: message, thread-directed raise,
/// DFL-reset + unblock + second raise, `exit_group(134)` fallback).
///
/// The raise is thread-directed (`tgkill` to self): process-directed
/// `kill` may route to another thread and return while this thread
/// proceeds to the fallback, racing the delivery. Thread-directed
/// fatal delivery to the calling thread is synchronous.
#[cfg(target_os = "linux")]
pub fn stack_chk_fail() -> ! {
    use crate::raw;
    const MSG: &[u8] = b"mol: stack smashing detected\n";
    const SIGABRT: u64 = 6; // same on both OSes
    let _ = raw::write(2, MSG.as_ptr() as u64, MSG.len() as u64);
    let tgid = raw::getpid();
    let tid = raw::gettid();
    let _ = raw::tgkill(tgid, tid, SIGABRT);
    // A caught-and-returned SIGABRT lands here: reset to DFL,
    // unblock, raise again.
    crate::signal::force_default(SIGABRT as i32);
    crate::signal::unblock(SIGABRT as i32);
    let _ = raw::tgkill(tgid, tid, SIGABRT);
    raw::exit_group(128 + 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_settable() {
        GUARD.store(0x1234, Ordering::Relaxed);
        assert_eq!(guard(), 0x1234);
        GUARD.store(0, Ordering::Relaxed);
    }
}
