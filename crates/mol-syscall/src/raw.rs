// SPDX-License-Identifier: MIT OR Apache-2.0
//! Raw Linux syscalls for guest-facing handlers (doc 25: no libc).
//!
//! Numbers verified against `linux-libc-dev` headers on both arches
//! (`asm-generic/unistd.h`, `x86_64 unistd_64.h`). Every number here
//! is executed by the per-module conformance suites (P1.2.V1).

use core::arch::asm;

#[cfg(target_arch = "x86_64")]
mod nr {
    pub const READ: u64 = 0;
    pub const WRITE: u64 = 1;
    pub const CLOSE: u64 = 3;
    pub const MMAP: u64 = 9;
    pub const MPROTECT: u64 = 10;
    pub const MUNMAP: u64 = 11;
    pub const RT_SIGACTION: u64 = 13;
    pub const RT_SIGPROCMASK: u64 = 14;
    pub const GETPID: u64 = 39;
    pub const CLONE: u64 = 56;
    pub const EXECVE: u64 = 59;
    pub const WAIT4: u64 = 61;
    pub const KILL: u64 = 62;
    pub const FLOCK: u64 = 73;
    pub const SIGALTSTACK: u64 = 131;
    pub const GETTID: u64 = 186;
    pub const EXIT_GROUP: u64 = 231;
    pub const TGKILL: u64 = 234;
    pub const OPENAT: u64 = 257;
    pub const PIPE2: u64 = 293;
    pub const GETRANDOM: u64 = 318;
}

#[cfg(target_arch = "aarch64")]
mod nr {
    pub const FLOCK: u64 = 32;
    pub const OPENAT: u64 = 56;
    pub const CLOSE: u64 = 57;
    pub const PIPE2: u64 = 59;
    pub const READ: u64 = 63;
    pub const WRITE: u64 = 64;
    pub const EXIT_GROUP: u64 = 94;
    pub const KILL: u64 = 129;
    pub const TGKILL: u64 = 131;
    pub const SIGALTSTACK: u64 = 132;
    pub const RT_SIGACTION: u64 = 134;
    pub const RT_SIGPROCMASK: u64 = 135;
    pub const GETPID: u64 = 172;
    pub const GETTID: u64 = 178;
    pub const MUNMAP: u64 = 215;
    pub const CLONE: u64 = 220;
    pub const EXECVE: u64 = 221;
    pub const MMAP: u64 = 222;
    pub const MPROTECT: u64 = 226;
    pub const WAIT4: u64 = 260;
    pub const GETRANDOM: u64 = 278;
}

#[cfg(target_arch = "x86_64")]
unsafe fn syscall6(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    let ret: i64;
    // SAFETY: caller-supplied ABI-correct arguments; `syscall`
    // clobbers rcx/r11 per the kernel contract.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") n as i64 => ret,
            in("rdi") a1,
            in("rsi") a2,
            in("rdx") a3,
            in("r10") a4,
            in("r8") a5,
            in("r9") a6,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

#[cfg(target_arch = "aarch64")]
unsafe fn syscall6(n: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    let ret: i64;
    // SAFETY: as above; `svc` preserves the general registers
    // except the x0 return value.
    unsafe {
        asm!(
            "svc #0",
            inlateout("x0") a1 => ret,
            in("x1") a2,
            in("x2") a3,
            in("x3") a4,
            in("x4") a5,
            in("x5") a6,
            in("x8") n,
            options(nostack, preserves_flags)
        );
    }
    ret
}

fn check(ret: i64) -> Result<u64, i32> {
    if (-4095..0).contains(&ret) {
        Err(-ret as i32)
    } else {
        Ok(ret as u64)
    }
}

pub const AT_FDCWD: i64 = -100;

/// `openat`-only: arm64 has no `open`, so both arches share it.
pub fn openat(dirfd: i64, path: u64, flags: u64, mode: u64) -> Result<u64, i32> {
    // SAFETY: path points at a NUL-terminated guest string; the
    // kernel copies it in (EFAULT on a wild pointer, same as native
    // modulo our bounded pre-scan, doc 43).
    check(unsafe { syscall6(nr::OPENAT, dirfd as u64, path, flags, mode, 0, 0) })
}

pub fn read(fd: i64, buf: u64, len: u64) -> Result<u64, i32> {
    // SAFETY: buf/len describe the guest's buffer; single attempt,
    // no userspace restart (matches the libc wrapper).
    check(unsafe { syscall6(nr::READ, fd as u64, buf, len, 0, 0, 0) })
}

pub fn write(fd: i64, buf: u64, len: u64) -> Result<u64, i32> {
    // SAFETY: as for read.
    check(unsafe { syscall6(nr::WRITE, fd as u64, buf, len, 0, 0, 0) })
}

pub fn close(fd: i64) -> Result<u64, i32> {
    // SAFETY: fd-only.
    check(unsafe { syscall6(nr::CLOSE, fd as u64, 0, 0, 0, 0, 0) })
}

pub fn mmap(addr: u64, len: u64, prot: u64, flags: u64, fd: i64, off: u64) -> Result<u64, i32> {
    // SAFETY: anonymous or guest-fd-backed mapping in our own
    // address space; invalid ranges fail cleanly.
    check(unsafe { syscall6(nr::MMAP, addr, len, prot, flags, fd as u64, off) })
}

pub fn munmap(addr: u64, len: u64) -> Result<u64, i32> {
    // SAFETY: affects only our address space; invalid ranges fail.
    check(unsafe { syscall6(nr::MUNMAP, addr, len, 0, 0, 0, 0) })
}

pub fn mprotect(addr: u64, len: u64, prot: u64) -> Result<u64, i32> {
    // SAFETY: as for munmap.
    check(unsafe { syscall6(nr::MPROTECT, addr, len, prot, 0, 0, 0) })
}

/// Raw `rt_sigaction`. `act`/`oldact` point at the kernel
/// `struct sigaction` layout (handler, flags, restorer, mask).
pub fn rt_sigaction(sig: u64, act: u64, oldact: u64, sigsetsize: u64) -> Result<u64, i32> {
    // SAFETY: pointers are either null or valid kernel-sigaction
    // structs owned by the caller.
    check(unsafe { syscall6(nr::RT_SIGACTION, sig, act, oldact, sigsetsize, 0, 0) })
}

pub fn getpid() -> u64 {
    // SAFETY: infallible.
    unsafe { syscall6(nr::GETPID, 0, 0, 0, 0, 0, 0) as u64 }
}

pub fn gettid() -> u64 {
    // SAFETY: infallible.
    unsafe { syscall6(nr::GETTID, 0, 0, 0, 0, 0, 0) as u64 }
}

/// Thread-directed signal (the `raise` primitive). Fatal signals to
/// the calling thread are delivered synchronously — unlike
/// process-directed `kill`, which the kernel may route to another
/// thread, racing any fallback after it.
pub fn tgkill(tgid: u64, tid: u64, sig: u64) -> Result<u64, i32> {
    // SAFETY: id/signal only.
    check(unsafe { syscall6(nr::TGKILL, tgid, tid, sig, 0, 0, 0) })
}

/// `sigaltstack(ss, old_ss)`.
pub fn sigaltstack(ss: u64, old_ss: u64) -> Result<u64, i32> {
    // SAFETY: pointers are null or valid `stack_t` structs.
    check(unsafe { syscall6(nr::SIGALTSTACK, ss, old_ss, 0, 0, 0, 0) })
}

pub const SIG_UNBLOCK: u64 = 1;

/// `rt_sigprocmask(how, set, oldset, sigsetsize)`. Sets are single
/// `u64` words (`sigsetsize` 8).
pub fn rt_sigprocmask(how: u64, set: u64, oldset: u64) -> Result<u64, i32> {
    // SAFETY: pointers are null or valid single-word sets.
    check(unsafe { syscall6(nr::RT_SIGPROCMASK, how, set, oldset, 8, 0, 0) })
}

pub fn kill(pid: i64, sig: u64) -> Result<u64, i32> {
    // SAFETY: pid/sig only.
    check(unsafe { syscall6(nr::KILL, pid as u64, sig, 0, 0, 0, 0) })
}

/// `clone` with fork semantics when `stack` is 0 and `flags` is
/// `SIGCHLD` (17).
pub fn clone(flags: u64, stack: u64, ptid: u64, ctid: u64, tls: u64) -> Result<u64, i32> {
    // SAFETY: fork-equivalent arguments; the child returns 0 and must
    // only run async-signal-safe code until exec (enforced by the
    // spawn path, which touches nothing but raw syscalls + argv).
    check(unsafe { syscall6(nr::CLONE, flags, stack, ptid, ctid, tls, 0) })
}

pub fn execve(path: u64, argv: u64, envp: u64) -> Result<u64, i32> {
    // SAFETY: NUL-terminated string vectors in our address space;
    // returns only on failure.
    check(unsafe { syscall6(nr::EXECVE, path, argv, envp, 0, 0, 0) })
}

pub fn wait4(pid: i64, status: u64, options: u64) -> Result<u64, i32> {
    // SAFETY: status is null or a valid out-pointer; rusage NULL.
    check(unsafe { syscall6(nr::WAIT4, pid as u64, status, options, 0, 0, 0) })
}

pub fn pipe2(fds: u64, flags: u64) -> Result<u64, i32> {
    // SAFETY: fds points at two writable ints owned by the caller.
    check(unsafe { syscall6(nr::PIPE2, fds, flags, 0, 0, 0, 0) })
}

pub const LOCK_SH: u64 = 1;
pub const LOCK_EX: u64 = 2;

pub fn flock(fd: i64, op: u64) -> Result<u64, i32> {
    // SAFETY: fd-only.
    check(unsafe { syscall6(nr::FLOCK, fd as u64, op, 0, 0, 0, 0) })
}

pub fn exit_group(status: i32) -> ! {
    // SAFETY: diverges.
    unsafe {
        syscall6(nr::EXIT_GROUP, status as u64, 0, 0, 0, 0, 0);
        core::hint::unreachable_unchecked()
    }
}

pub fn getrandom(buf: &mut [u8]) -> Result<(), i32> {
    let mut done = 0usize;
    while done < buf.len() {
        let ptr = buf[done..].as_mut_ptr() as u64;
        let left = (buf.len() - done) as u64;
        // SAFETY: pointer + length describe a valid writable slice.
        let n = check(unsafe { syscall6(nr::GETRANDOM, ptr, left, 0, 0, 0, 0) })? as usize;
        if n == 0 {
            return Err(5); // EIO: no progress
        }
        done += n;
    }
    Ok(())
}

// `rt_sigreturn` trampoline for x86_64 signal delivery: the kernel
// has no return trampoline of its own, so `SA_RESTORER` must point
// here. arm64 returns via the kernel's own trampoline and never
// calls this. (Intel syntax: x86_64 asm! defaults to it.)
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".globl mol_sig_restorer",
    "mol_sig_restorer:",
    "mov rax, 15", // __NR_rt_sigreturn
    "syscall",
    ".size mol_sig_restorer, .-mol_sig_restorer",
);

unsafe extern "C" {
    /// Address of the x86_64 restorer above (Linux-only). Only its
    /// address is ever taken; it is never called from Rust.
    #[cfg(target_arch = "x86_64")]
    pub fn mol_sig_restorer();
}
