// SPDX-License-Identifier: MIT OR Apache-2.0
//! `--debug-catch-faults`: diagnose guest crashes. Installs
//! `SA_SIGINFO` handlers for `SIGSEGV`/`SIGBUS`/`SIGILL`/`SIGFPE`
//! that print the signal, code, fault address, and PC to stderr,
//! then re-raise with the default disposition so the exit status
//! still reports the signal. Raw syscalls only; the handler is
//! async-signal-safe (no allocation, manual hex).
//!
//! Layout facts (stable kernel ABI): `siginfo_t` has `si_code` at
//! +8 and `si_addr` at +16 on both arches. x86_64 `ucontext_t` is
//! flags + link + `stack_t` (40 bytes), then `mcontext_t` holding
//! `gregs[23]` with RIP at index 16 (PC at +168). aarch64 inserts
//! the 128-byte mask area first (40 + 128 = 168), then
//! `fault_address` + `regs[31]` + `sp` + `pc` (PC at +432).

#[cfg(target_os = "linux")]
use core::ffi::c_int;

// Linux `sa_flags` bits (<asm-generic/signal.h>, stable ABI).
#[cfg(target_os = "linux")]
const L_SA_SIGINFO: u64 = 0x0000_0004;
#[cfg(target_os = "linux")]
const L_SA_RESTORER: u64 = 0x0400_0000;

#[cfg(target_os = "linux")]
#[repr(C)]
struct KernelSigaction {
    handler: u64,
    flags: u64,
    restorer: u64,
    mask: u64,
}

#[cfg(target_arch = "x86_64")]
const PC_OFF: usize = 168;
/// Empirical: the faulting guest PC lands here (pinned by
/// `fault_catcher_reports_segv` against a known `ldr`), 8 past the
/// naive UAPI reading — the mask area runs 136, not 128, in the
/// delivered frame.
#[cfg(target_arch = "aarch64")]
const PC_OFF: usize = 440;

/// Write the whole buffer to fd 2, retrying short writes. Handler-safe.
#[cfg(target_os = "linux")]
fn write_all(mut ptr: u64, mut len: u64) {
    while len > 0 {
        match crate::sys_linux::write(2, ptr, len) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                ptr += n;
                len -= n;
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn emit(msg: &[u8]) {
    write_all(msg.as_ptr() as u64, msg.len() as u64);
}

#[cfg(target_os = "linux")]
fn emit_hex(mut v: u64) {
    let mut buf = [b'0'; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in (2..18).rev() {
        let d = (v & 0xf) as u8;
        buf[i] = if d < 10 { b'0' + d } else { b'a' + d - 10 };
        v >>= 4;
    }
    emit(&buf);
}

#[cfg(target_os = "linux")]
fn emit_dec(mut v: u64) {
    if v == 0 {
        emit(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut n = 0;
    while v > 0 {
        buf[n] = b'0' + (v % 10) as u8;
        v /= 10;
        n += 1;
    }
    let mut i = n;
    while i > 0 {
        i -= 1;
        emit(&buf[i..i + 1]);
    }
}

#[cfg(target_os = "linux")]
unsafe extern "C" fn fault_handler(sig: c_int, info: *mut u8, ctx: *mut u8) {
    // SAFETY: the kernel provides valid info/ctx for SA_SIGINFO
    // delivery; all reads are in-bounds by ABI layout.
    unsafe {
        let code = (info.add(8) as *const i32).read_unaligned();
        let addr = (info.add(16) as *const u64).read_unaligned();
        let pc = (ctx.add(PC_OFF) as *const u64).read_unaligned();
        emit(b"mol-loader: fatal signal ");
        emit_dec(sig as u64);
        emit(b" code ");
        emit_dec(code as u64);
        emit(b" addr ");
        emit_hex(addr);
        emit(b" pc ");
        emit_hex(pc);
        emit(b"\n");
        // Restore the default disposition, unblock the signal
        // (the kernel blocks it on handler entry — without this
        // the re-raise below only pends and we'd exit 128+sig
        // instead of dying by the signal), and re-raise.
        let dfl = KernelSigaction {
            handler: 0,
            flags: 0,
            restorer: 0,
            mask: 0,
        };
        let _ =
            crate::sys_linux::rt_sigaction(sig as u64, &dfl as *const KernelSigaction as u64, 0);
        let mask = 1u64 << (sig as u64 - 1);
        let _ = crate::sys_linux::rt_sigprocmask(
            crate::sys_linux::SIG_UNBLOCK,
            &mask as *const u64 as u64,
            0,
        );
        let _ = crate::sys_linux::kill(crate::sys_linux::getpid(), sig as u64);
        crate::sys_linux::exit_group(128 + sig as u64);
    }
}

/// Install the fault catcher. Call after libSystem init (which
/// resets `SIGBUS`/`SIGSEGV` to default) and before guest entry.
#[cfg(target_os = "linux")]
pub fn install() {
    // SIGILL, SIGBUS, SIGFPE, SIGSEGV (Linux numbers).
    for sig in [4u64, 7, 8, 11] {
        #[cfg(target_arch = "x86_64")]
        let (flags, restorer) = (
            L_SA_SIGINFO | L_SA_RESTORER,
            crate::sys_linux::mol_loader_sig_restorer as *const () as u64,
        );
        #[cfg(not(target_arch = "x86_64"))]
        let (flags, restorer) = (L_SA_SIGINFO, 0);
        let kact = KernelSigaction {
            handler: fault_handler as *const () as u64,
            flags,
            restorer,
            mask: 0,
        };
        let _ = crate::sys_linux::rt_sigaction(sig, &kact as *const KernelSigaction as u64, 0);
    }
}
