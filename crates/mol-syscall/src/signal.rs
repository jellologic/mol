// SPDX-License-Identifier: MIT OR Apache-2.0
//! Darwin→Linux signal translation + `sigaction` install/delivery.
//!
//! Signal numbers differ past 6 (notably `BUS` 10→7, `SYS` 12→31,
//! `USR1`/`USR2` 30/31→10/12); flags differ entirely. Guests install
//! plain (`sa_handler`) dispositions; the kernel delivers to a mol
//! trampoline that renumbers back to Darwin before tail-calling the
//! guest handler. `SA_SIGINFO` is rejected in the smoke subset: the
//! `siginfo`/`ucontext` layouts have no faithful 1:1 map (doc 43).

#[cfg(target_os = "linux")]
use core::sync::atomic::{AtomicU64, Ordering};

use crate::errno::{EINVAL, EIO};

// --- Darwin signals (sys/signal.h) ---

pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
#[allow(dead_code)]
pub const SIGPOLL: i32 = 7;
#[allow(dead_code)]
pub const SIGEMT: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGBUS: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGSYS: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGURG: i32 = 16;
pub const SIGSTOP: i32 = 17;
pub const SIGTSTP: i32 = 18;
pub const SIGCONT: i32 = 19;
pub const SIGCHLD: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGIO: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGINFO: i32 = 29;
pub const SIGUSR1: i32 = 30;
pub const SIGUSR2: i32 = 31;

/// Highest Darwin signal; Darwin has no realtime range.
pub const NSIG_DARWIN: i32 = 32;

// --- Darwin sa_flags (sys/signal.h) ---

pub const SA_ONSTACK: i32 = 0x0001;
pub const SA_RESTART: i32 = 0x0002;
pub const SA_RESETHAND: i32 = 0x0004;
pub const SA_NOCLDSTOP: i32 = 0x0008;
pub const SA_NODEFER: i32 = 0x0010;
pub const SA_NOCLDWAIT: i32 = 0x0020;
pub const SA_SIGINFO: i32 = 0x0040;
pub const SA_USERTRAMP: i32 = 0x0100;
pub const SA_64REGSET: i32 = 0x0200;

// --- Linux sa_flags (asm-generic/signal-defs.h) ---

const L_SA_NOCLDSTOP: u64 = 0x0000_0001;
const L_SA_NOCLDWAIT: u64 = 0x0000_0002;
const L_SA_SIGINFO: u64 = 0x0000_0004;
const L_SA_ONSTACK: u64 = 0x0800_0000;
const L_SA_RESTART: u64 = 0x1000_0000;
const L_SA_NODEFER: u64 = 0x4000_0000;
const L_SA_RESETHAND: u64 = 0x8000_0000;
#[cfg_attr(
    not(all(target_os = "linux", target_arch = "x86_64")),
    allow(dead_code)
)]
const L_SA_RESTORER: u64 = 0x0400_0000;

/// Guest-visible `struct sigaction`: handler + u32 mask + flags.
/// 16 bytes, both arches.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DarwinSigaction {
    pub handler: u64,
    pub mask: u32,
    pub flags: i32,
}

pub const SIG_DFL: u64 = 0;
pub const SIG_IGN: u64 = 1;

/// Darwin signal → Linux signal. `SIGPOLL`(7) and `SIGIO`(23) both
/// land on Linux 29 (`SIGIO`/`SIGPOLL` are one signal there too), so
/// installing either replaces the other — the same aliasing Linux
/// itself exhibits. `SIGINFO`(29, the `^T` status key) has no Linux
/// cousin and fails with `EINVAL`.
pub fn signo_to_linux(signo: i32) -> Result<i32, i32> {
    Ok(match signo {
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 6,
        7 => 29,
        8 => 8,
        9 => 9,
        10 => 7,
        11 => 11,
        12 => 31,
        13 => 13,
        14 => 14,
        15 => 15,
        16 => 23,
        17 => 19,
        18 => 20,
        19 => 18,
        20 => 17,
        21 => 21,
        22 => 22,
        23 => 29,
        24 => 24,
        25 => 25,
        26 => 26,
        27 => 27,
        28 => 28,
        29 => return Err(EINVAL),
        30 => 10,
        31 => 12,
        _ => return Err(EINVAL),
    })
}

/// Linux signal → Darwin signal (delivery path). Inverse of
/// [`signo_to_linux`] over installed signals; Linux 29 reports as
/// `SIGIO`(23), the live name of the aliased pair.
pub fn signo_from_linux(signo: i32) -> Result<i32, i32> {
    Ok(match signo {
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 6,
        7 => 10,
        8 => 8,
        9 => 9,
        10 => 30,
        11 => 11,
        12 => 31,
        13 => 13,
        14 => 14,
        15 => 15,
        17 => 20,
        18 => 19,
        19 => 17,
        20 => 18,
        21 => 21,
        22 => 22,
        23 => 16,
        24 => 24,
        25 => 25,
        26 => 26,
        27 => 27,
        28 => 28,
        29 => 23,
        30 => return Err(EIO), // SIGPWR/SIGSTKFLT: no Darwin cousin
        31 => 12,
        _ => return Err(EIO),
    })
}

/// Darwin `sa_flags` → Linux `sa_flags`. `SA_SIGINFO` is rejected
/// (smoke boundary, see module doc); `SA_USERTRAMP` is accepted and
/// ignored (Linux delivery is already tramp-free for plain
/// handlers); `SA_64REGSET` is accepted and ignored (a `SA_SIGINFO`
/// modifier, meaningless without it); anything else fails `EINVAL`.
pub fn flags_to_linux(flags: i32) -> Result<u64, i32> {
    let f = flags as u32;
    if f & (SA_SIGINFO as u32) != 0 {
        return Err(EINVAL);
    }
    let known = (SA_ONSTACK
        | SA_RESTART
        | SA_RESETHAND
        | SA_NOCLDSTOP
        | SA_NODEFER
        | SA_NOCLDWAIT
        | SA_USERTRAMP
        | SA_64REGSET) as u32;
    if f & !known != 0 {
        return Err(EINVAL);
    }
    let mut out = 0u64;
    if f & (SA_ONSTACK as u32) != 0 {
        out |= L_SA_ONSTACK;
    }
    if f & (SA_RESTART as u32) != 0 {
        out |= L_SA_RESTART;
    }
    if f & (SA_RESETHAND as u32) != 0 {
        out |= L_SA_RESETHAND;
    }
    if f & (SA_NOCLDSTOP as u32) != 0 {
        out |= L_SA_NOCLDSTOP;
    }
    if f & (SA_NODEFER as u32) != 0 {
        out |= L_SA_NODEFER;
    }
    if f & (SA_NOCLDWAIT as u32) != 0 {
        out |= L_SA_NOCLDWAIT;
    }
    Ok(out)
}

/// Linux `sa_flags` → Darwin `sa_flags` (oldact path). `SA_RESTORER`
/// and `SA_SIGINFO` are stripped: the restorer is ours, and mol never
/// installs `SA_SIGINFO` itself.
pub fn flags_from_linux(flags: u64) -> i32 {
    let mut out = 0u32;
    if flags & L_SA_ONSTACK != 0 {
        out |= SA_ONSTACK as u32;
    }
    if flags & L_SA_RESTART != 0 {
        out |= SA_RESTART as u32;
    }
    if flags & L_SA_RESETHAND != 0 {
        out |= SA_RESETHAND as u32;
    }
    if flags & L_SA_NOCLDSTOP != 0 {
        out |= SA_NOCLDSTOP as u32;
    }
    if flags & L_SA_NODEFER != 0 {
        out |= SA_NODEFER as u32;
    }
    if flags & L_SA_NOCLDWAIT != 0 {
        out |= SA_NOCLDWAIT as u32;
    }
    let _ = L_SA_SIGINFO;
    out as i32
}

/// Darwin mask word → Linux mask word, bit by bit over 1..=31.
pub fn mask_to_linux(mask: u32) -> u64 {
    let mut out = 0u64;
    for darwin in 1..NSIG_DARWIN {
        if mask & (1u32 << (darwin - 1)) != 0 {
            if let Ok(linux) = signo_to_linux(darwin) {
                out |= 1u64 << (linux - 1);
            }
        }
    }
    out
}

/// Linux mask word → Darwin mask word over the mapped range.
pub fn mask_from_linux(mask: u64) -> u32 {
    let mut out = 0u32;
    for linux in 1..=31 {
        if mask & (1u64 << (linux - 1)) != 0 {
            if let Ok(darwin) = signo_from_linux(linux) {
                out |= 1u32 << (darwin - 1);
            }
        }
    }
    out
}

/// Default `sa_flags` template per signal, measured on macOS
/// (oracle `sigdef`: `HUP`/`TERM`/`WINCH` report `0x42`,
/// `BUS`/`SEGV` report `0x40`, most others `0x2`, a few `0x0`).
/// Reported for never-installed dispositions instead of translating
/// Linux's own template (whose `SA_RESTORER` bit would leak).
pub fn default_flags(signo: i32) -> i32 {
    match signo {
        SIGHUP | SIGTERM | SIGWINCH => 0x42,
        SIGBUS | SIGSEGV => 0x40,
        SIGINT | SIGQUIT | SIGALRM | SIGTSTP | SIGCHLD | SIGTTIN | SIGTTOU => 0x0,
        _ => 0x2,
    }
}

// --- install + delivery (Linux-only) ---

/// Guest handler by Linux signo, installed through the trampoline.
/// Index 0 unused; `SIG_DFL`/`SIG_IGN` mean the guest set a
/// non-catching disposition through us.
#[cfg(target_os = "linux")]
static HANDLERS: [AtomicU64; 32] = [const { AtomicU64::new(SIG_DFL) }; 32];

/// Whether mol installed the current disposition (vs inherited or
/// loader-installed), by Linux signo.
#[cfg(target_os = "linux")]
static OURS: [AtomicU64; 32] = [const { AtomicU64::new(0) }; 32];

/// Delivery trampoline: renumber to Darwin, tail-call the guest.
#[cfg(target_os = "linux")]
extern "C" fn trampoline(linux_signo: i32) {
    let idx = linux_signo as usize;
    if idx >= HANDLERS.len() {
        return;
    }
    let guest = HANDLERS[idx].load(Ordering::SeqCst);
    if guest == SIG_DFL || guest == SIG_IGN {
        return;
    }
    let darwin = signo_from_linux(linux_signo).unwrap_or(linux_signo);
    // SAFETY: the guest installed this address as its handler; the
    // plain-handler ABI is one int arg on both arches.
    unsafe {
        let f: extern "C" fn(i32) = core::mem::transmute(guest as usize);
        f(darwin);
    }
}

#[cfg(target_os = "linux")]
fn trampoline_addr() -> u64 {
    trampoline as *const () as u64
}

/// Reset a Linux signal to `SIG_DFL` and clear mol's bookkeeping
/// (the `abort` path's second raise; glibc parity).
#[cfg(target_os = "linux")]
pub(crate) fn force_default(linux_signo: i32) {
    let kact = KernelSigaction {
        handler: SIG_DFL,
        flags: 0,
        restorer: 0,
        mask: 0,
    };
    let _ = crate::raw::rt_sigaction(
        linux_signo as u64,
        &kact as *const KernelSigaction as u64,
        0,
        8,
    );
    let idx = linux_signo as usize;
    if idx < HANDLERS.len() {
        HANDLERS[idx].store(SIG_DFL, Ordering::SeqCst);
        OURS[idx].store(0, Ordering::SeqCst);
    }
}

/// Disable the calling thread's alternate signal stack
/// (`SS_DISABLE`). Used with [`reset_runtime_handlers`].
#[cfg(target_os = "linux")]
pub(crate) fn disable_altstack() {
    #[repr(C)]
    struct AltStack {
        sp: u64,
        flags: i32,
        _pad: i32,
        size: u64,
    }
    const SS_DISABLE: i32 = 2;
    let ss = AltStack {
        sp: 0,
        flags: SS_DISABLE,
        _pad: 0,
        size: 0,
    };
    let _ = crate::raw::sigaltstack(&ss as *const AltStack as u64, 0);
}

/// Reset the Rust runtime's own signal state before guests run: the
/// runtime catches Linux `SIGBUS`(7)/`SIGSEGV`(11) for stack-overflow
/// detection and installs an alternate stack. Guests must observe
/// pristine `SIG_DFL` dispositions (oracle template) and no alt
/// stack. The loader calls this in `__libkernel_init`.
#[cfg(target_os = "linux")]
pub fn reset_runtime_handlers() {
    force_default(7); // SIGBUS
    force_default(11); // SIGSEGV
    disable_altstack();
}

/// Unblock one Linux signal on the calling thread.
#[cfg(target_os = "linux")]
pub(crate) fn unblock(linux_signo: i32) {
    let mask = 1u64 << (linux_signo - 1);
    let _ = crate::raw::rt_sigprocmask(crate::raw::SIG_UNBLOCK, &mask as *const u64 as u64, 0);
}

/// Kernel `struct sigaction` (handler, flags, restorer, mask).
#[cfg(target_os = "linux")]
#[repr(C)]
struct KernelSigaction {
    handler: u64,
    flags: u64,
    restorer: u64,
    mask: u64,
}

/// Install or query a disposition. `act`/`oldact` use guest layout;
/// a null `act` queries only. Returns a Darwin errno on failure.
#[cfg(target_os = "linux")]
pub fn sigaction(
    signo: i32,
    act: Option<&DarwinSigaction>,
    oldact: Option<&mut DarwinSigaction>,
) -> Result<(), i32> {
    use crate::errno::from_linux;
    use crate::raw;

    if signo <= 0 || signo >= NSIG_DARWIN {
        return Err(EINVAL);
    }
    // KILL/STOP can neither be caught, ignored, nor queried:
    // EINVAL on macOS for all cases (query verified by oracle) and
    // EINVAL from Linux `rt_sigaction` likewise.
    if signo == SIGKILL || signo == SIGSTOP {
        return Err(EINVAL);
    }
    let linux_signo = signo_to_linux(signo)?;

    let kact;
    let kact_ptr = if let Some(a) = act {
        let flags = flags_to_linux(a.flags)?;
        let handler = match a.handler {
            SIG_DFL => SIG_DFL,
            SIG_IGN => SIG_IGN,
            guest => {
                HANDLERS[linux_signo as usize].store(guest, Ordering::SeqCst);
                trampoline_addr()
            }
        };
        OURS[linux_signo as usize].store(1, Ordering::SeqCst);
        #[cfg(target_arch = "x86_64")]
        let (flags, restorer) = (
            flags | L_SA_RESTORER,
            raw::mol_sig_restorer as *const () as u64,
        );
        #[cfg(not(target_arch = "x86_64"))]
        let (flags, restorer) = (flags, 0u64);
        kact = KernelSigaction {
            handler,
            flags,
            restorer,
            mask: mask_to_linux(a.mask),
        };
        &kact as *const KernelSigaction as u64
    } else {
        0
    };
    let mut kold = KernelSigaction {
        handler: 0,
        flags: 0,
        restorer: 0,
        mask: 0,
    };
    let kold_ptr = if oldact.is_some() {
        &mut kold as *mut KernelSigaction as u64
    } else {
        0
    };
    raw::rt_sigaction(linux_signo as u64, kact_ptr, kold_ptr, 8).map_err(from_linux)?;
    if let Some(old) = oldact {
        old.handler = match kold.handler {
            0 => SIG_DFL,
            1 => SIG_IGN,
            h if h == trampoline_addr() => HANDLERS[linux_signo as usize].load(Ordering::SeqCst),
            h => h, // foreign (loader-inherited) handler: raw address
        };
        old.mask = mask_from_linux(kold.mask);
        old.flags = if kold.handler == 0 && OURS[linux_signo as usize].load(Ordering::SeqCst) == 0 {
            // Never installed through us: report Darwin's default
            // template, not Linux's (which carries SA_RESTORER).
            default_flags(signo)
        } else {
            flags_from_linux(kold.flags)
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_spot_checks() {
        assert_eq!(signo_to_linux(SIGHUP), Ok(1));
        assert_eq!(signo_to_linux(SIGBUS), Ok(7));
        assert_eq!(signo_to_linux(SIGSYS), Ok(31));
        assert_eq!(signo_to_linux(SIGURG), Ok(23));
        assert_eq!(signo_to_linux(SIGSTOP), Ok(19));
        assert_eq!(signo_to_linux(SIGCONT), Ok(18));
        assert_eq!(signo_to_linux(SIGCHLD), Ok(17));
        assert_eq!(signo_to_linux(SIGIO), Ok(29));
        assert_eq!(signo_to_linux(SIGPOLL), Ok(29)); // aliased pair
        assert_eq!(signo_to_linux(SIGUSR1), Ok(10));
        assert_eq!(signo_to_linux(SIGUSR2), Ok(12));
        assert_eq!(signo_to_linux(SIGINFO), Err(EINVAL));
        assert_eq!(signo_to_linux(0), Err(EINVAL));
        assert_eq!(signo_to_linux(32), Err(EINVAL));
    }

    #[test]
    fn number_roundtrip_over_installed_range() {
        // Every installable Darwin signal survives Linux and back,
        // except the POLL alias which canonically returns IO.
        for darwin in 1..NSIG_DARWIN {
            if darwin == SIGINFO {
                continue;
            }
            let linux = signo_to_linux(darwin).unwrap();
            let back = signo_from_linux(linux).unwrap();
            if darwin == SIGPOLL {
                assert_eq!(back, SIGIO);
            } else {
                assert_eq!(back, darwin, "signal {darwin}");
            }
        }
    }

    #[test]
    fn flag_translation() {
        assert_eq!(
            flags_to_linux(SA_ONSTACK | SA_RESTART | SA_RESETHAND).unwrap(),
            L_SA_ONSTACK | L_SA_RESTART | L_SA_RESETHAND
        );
        assert_eq!(
            flags_to_linux(SA_NOCLDSTOP | SA_NOCLDWAIT | SA_NODEFER).unwrap(),
            L_SA_NOCLDSTOP | L_SA_NOCLDWAIT | L_SA_NODEFER
        );
        // USERTRAMP/64REGSET accepted, ignored.
        assert_eq!(flags_to_linux(SA_USERTRAMP | SA_64REGSET), Ok(0));
        // SIGINFO rejected; unknown bits rejected.
        assert_eq!(flags_to_linux(SA_SIGINFO), Err(EINVAL));
        assert_eq!(flags_to_linux(0x0400), Err(EINVAL));
        // And back (restorer stripped).
        assert_eq!(
            flags_from_linux(L_SA_RESTART | L_SA_RESTORER | L_SA_SIGINFO),
            SA_RESTART
        );
    }

    #[test]
    fn mask_roundtrip() {
        let darwin = (1u32 << (SIGUSR1 - 1)) | (1u32 << (SIGTERM - 1));
        let linux = mask_to_linux(darwin);
        assert_eq!(linux, (1u64 << 9) | (1u64 << 14)); // USR1=10, TERM=15
        assert_eq!(mask_from_linux(linux), darwin);
        assert_eq!(mask_to_linux(0), 0);
    }

    #[test]
    fn default_flag_template_matches_oracle() {
        assert_eq!(default_flags(SIGHUP), 0x42);
        assert_eq!(default_flags(SIGTERM), 0x42);
        assert_eq!(default_flags(SIGWINCH), 0x42);
        assert_eq!(default_flags(SIGBUS), 0x40);
        assert_eq!(default_flags(SIGSEGV), 0x40);
        assert_eq!(default_flags(SIGINT), 0x0);
        assert_eq!(default_flags(SIGCHLD), 0x0);
        assert_eq!(default_flags(SIGUSR1), 0x2);
        assert_eq!(default_flags(SIGPIPE), 0x2);
    }

    #[test]
    fn guest_sigaction_layout_is_16_bytes() {
        assert_eq!(core::mem::size_of::<DarwinSigaction>(), 16);
        assert_eq!(core::mem::align_of::<DarwinSigaction>(), 8);
    }
}
