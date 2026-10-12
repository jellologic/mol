// SPDX-License-Identifier: MIT OR Apache-2.0
//! Hand-rolled Linux syscalls for the loader (doc 25: no libc).
//!
//! Raw `syscall`/`svc` wrappers for x86_64 and AArch64 covering only
//! what mapping + process control + fault diagnostics need:
//! `mmap`, `mprotect`, `munmap`, `getrandom`, `exit_group`,
//! `write`, `rt_sigaction`, `kill`, `getpid`. Guest-facing
//! emulation builds on this module in P1.2.

use core::arch::asm;

pub const PROT_NONE: u64 = 0;
pub const PROT_READ: u64 = 1;
pub const PROT_WRITE: u64 = 2;
pub const PROT_EXEC: u64 = 4;

pub const MAP_PRIVATE: u64 = 0x02;
pub const MAP_FIXED: u64 = 0x10;
pub const MAP_ANONYMOUS: u64 = 0x20;
pub const MAP_FIXED_NOREPLACE: u64 = 0x100000;

#[cfg(target_arch = "x86_64")]
mod nr {
    pub const WRITE: u64 = 1;
    pub const MMAP: u64 = 9;
    pub const MPROTECT: u64 = 10;
    pub const MUNMAP: u64 = 11;
    pub const RT_SIGACTION: u64 = 13;
    pub const RT_SIGPROCMASK: u64 = 14;
    pub const KILL: u64 = 62;
    pub const GETPID: u64 = 39;
    pub const GETRANDOM: u64 = 318;
    pub const EXIT_GROUP: u64 = 231;
}

#[cfg(target_arch = "aarch64")]
mod nr {
    pub const WRITE: u64 = 64;
    pub const MMAP: u64 = 222;
    pub const MPROTECT: u64 = 226;
    pub const MUNMAP: u64 = 215;
    pub const RT_SIGACTION: u64 = 134;
    pub const RT_SIGPROCMASK: u64 = 135;
    pub const KILL: u64 = 129;
    pub const GETPID: u64 = 172;
    pub const GETRANDOM: u64 = 278;
    pub const EXIT_GROUP: u64 = 94;
}

/// Raw 6-arg syscall. Returns the kernel return value (negative
/// `-errno` on failure).
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

/// Raw 6-arg syscall (AArch64: number in x8, args in x0-x5).
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

/// Write to a file descriptor.
pub fn write(fd: u64, buf: u64, len: u64) -> Result<u64, i32> {
    // SAFETY: caller-supplied valid buffer.
    check(unsafe { syscall6(nr::WRITE, fd, buf, len, 0, 0, 0) })
}

/// Install a signal disposition (`sigsetsize` must be 8).
pub fn rt_sigaction(sig: u64, act: u64, oldact: u64) -> Result<u64, i32> {
    // SAFETY: caller-supplied valid sigaction structs.
    check(unsafe { syscall6(nr::RT_SIGACTION, sig, act, oldact, 8, 0, 0) })
}

/// Send a signal to a process.
pub fn kill(pid: u64, sig: u64) -> Result<u64, i32> {
    // SAFETY: numbers only.
    check(unsafe { syscall6(nr::KILL, pid, sig, 0, 0, 0, 0) })
}

/// `rt_sigprocmask` `how` to remove signals from the mask.
pub const SIG_UNBLOCK: u64 = 1;

/// Change the calling thread's signal mask (`sigsetsize` is 8).
pub fn rt_sigprocmask(how: u64, set: u64, oldset: u64) -> Result<u64, i32> {
    // SAFETY: caller-supplied valid mask pointers.
    check(unsafe { syscall6(nr::RT_SIGPROCMASK, how, set, oldset, 8, 0, 0) })
}

/// Calling process id.
pub fn getpid() -> u64 {
    // SAFETY: numbers only.
    unsafe { syscall6(nr::GETPID, 0, 0, 0, 0, 0, 0) as u64 }
}

/// Terminate the process group with a status. Diverges.
pub fn exit_group(status: u64) -> ! {
    // SAFETY: diverges.
    unsafe {
        syscall6(nr::EXIT_GROUP, status, 0, 0, 0, 0, 0);
        core::hint::unreachable_unchecked()
    }
}

// `rt_sigreturn` trampoline for x86_64 signal delivery (same shape
// as mol-syscall's): the kernel has no return trampoline of its
// own, so `SA_RESTORER` must point here. arm64 returns via the
// kernel's own trampoline and never calls this.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".globl mol_loader_sig_restorer",
    "mol_loader_sig_restorer:",
    "mov rax, 15", // __NR_rt_sigreturn
    "syscall",
    ".size mol_loader_sig_restorer, .-mol_loader_sig_restorer",
);

unsafe extern "C" {
    /// Address of the x86_64 restorer above. Only its address is
    /// ever taken; it is never called from Rust.
    #[cfg(target_arch = "x86_64")]
    pub fn mol_loader_sig_restorer();
}

/// Anonymous mapping anywhere the kernel chooses.
pub fn mmap_any(len: u64, prot: u64) -> Result<u64, i32> {
    // SAFETY: anonymous private mapping with a kernel-chosen address.
    check(unsafe {
        syscall6(
            nr::MMAP,
            0,
            len,
            prot,
            MAP_PRIVATE | MAP_ANONYMOUS,
            u64::from_ne_bytes((-1i64).to_ne_bytes()),
            0,
        )
    })
}

/// Anonymous mapping at a fixed address. `noreplace` fails with
/// `EEXIST` instead of clobbering an existing mapping.
pub fn mmap_fixed(addr: u64, len: u64, prot: u64, noreplace: bool) -> Result<u64, i32> {
    let mut flags = MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED;
    if noreplace {
        flags |= MAP_FIXED_NOREPLACE;
    }
    // SAFETY: anonymous private mapping; length > 0 checked by caller
    // contract (zero-length fails cleanly with EINVAL anyway).
    check(unsafe {
        syscall6(
            nr::MMAP,
            addr,
            len,
            prot,
            flags,
            u64::from_ne_bytes((-1i64).to_ne_bytes()),
            0,
        )
    })
}

pub fn mprotect(addr: u64, len: u64, prot: u64) -> Result<(), i32> {
    // SAFETY: affects only the caller's address space; invalid ranges
    // fail with ENOMEM instead of faulting.
    check(unsafe { syscall6(nr::MPROTECT, addr, len, prot, 0, 0, 0) }).map(|_| ())
}

pub fn munmap(addr: u64, len: u64) -> Result<(), i32> {
    // SAFETY: as for mprotect.
    check(unsafe { syscall6(nr::MUNMAP, addr, len, 0, 0, 0, 0) }).map(|_| ())
}

/// Fill `buf` with cryptographic randomness (ASLR slides).
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

/// Flush the instruction cache over `bytes` after writing guest code
/// (AArch64; x86_64 is cache-coherent for this purpose). Line sizes
/// come from `CTR_EL0`, readable at EL0.
pub fn clear_cache(bytes: &mut [u8]) {
    #[cfg(target_arch = "aarch64")]
    {
        let start = bytes.as_mut_ptr() as u64;
        let end = start.wrapping_add(bytes.len() as u64);
        // SAFETY: EL0 may read CTR_EL0 and issue DC CVAU / IC IVAU by
        // VA; the range is mapped in this process.
        unsafe {
            asm!(
                "mrs {ctr}, ctr_el0",
                // dsize = 4 << DminLine (CTR bits 19:16).
                "lsr {tmp}, {ctr}, #16",
                "and {tmp}, {tmp}, #15",
                "mov {sz}, #4",
                "lsl {sz}, {sz}, {tmp}",
                // align cursor down, clean data lines to PoU.
                "sub {mask}, {sz}, #1",
                "bic {cur}, {start}, {mask}",
                "2:",
                "dc cvau, {cur}",
                "add {cur}, {cur}, {sz}",
                "cmp {cur}, {end}",
                "b.lo 2b",
                "dsb ish",
                // isize = 4 << IminLine (CTR bits 3:0).
                "and {tmp}, {ctr}, #15",
                "mov {sz}, #4",
                "lsl {sz}, {sz}, {tmp}",
                "sub {mask}, {sz}, #1",
                "bic {cur}, {start}, {mask}",
                "3:",
                "ic ivau, {cur}",
                "add {cur}, {cur}, {sz}",
                "cmp {cur}, {end}",
                "b.lo 3b",
                "dsb ish",
                "isb",
                ctr = out(reg) _,
                tmp = out(reg) _,
                sz = out(reg) _,
                mask = out(reg) _,
                cur = out(reg) _,
                start = in(reg) start,
                end = in(reg) end,
                options(nostack)
            );
        }
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        let _ = bytes;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn mmap_write_mprotect_roundtrip() {
        let page = 0x1000u64;
        // Anonymous fixed mapping at a scratch address is unsafe to
        // guess; map anywhere (addr 0 without FIXED is a hint).
        let at = check(unsafe {
            syscall6(
                nr::MMAP,
                0,
                page,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                u64::from_ne_bytes((-1i64).to_ne_bytes()),
                0,
            )
        })
        .unwrap();
        // SAFETY: freshly mapped page, exclusively owned.
        unsafe {
            (at as *mut u64).write(0x1234_5678);
            assert_eq!((at as *const u64).read(), 0x1234_5678);
        }
        mprotect(at, page, PROT_READ).unwrap();
        // SAFETY: same mapping.
        unsafe {
            assert_eq!((at as *const u64).read(), 0x1234_5678);
        }
        munmap(at, page).unwrap();
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn getrandom_fills() {
        let mut buf = [0u8; 32];
        getrandom(&mut buf).unwrap();
        assert!(buf.iter().any(|&b| b != 0));
    }
}
