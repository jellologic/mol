// SPDX-License-Identifier: MIT OR Apache-2.0
//! `sigaction`/`kill` conformance (P1.2.V1): default templates,
//! install/query roundtrip, real delivery with Darwin numbering.
#![cfg(target_os = "linux")]

mod common;

use core::sync::atomic::{AtomicI32, Ordering};
use mol_syscall::errno::*;
use mol_syscall::signal::*;

static SEEN: AtomicI32 = AtomicI32::new(-1);

extern "C" fn probe(signo: i32) {
    SEEN.store(signo, Ordering::SeqCst);
}

#[test]
fn default_dispositions_match_oracle_templates() {
    // The loader runs this in __libkernel_init; the suite runs it so
    // the test process itself looks pristine (Rust catches BUS/SEGV).
    mol_syscall::signal::reset_runtime_handlers();
    // Never-installed signals report DFL + the Apple flag
    // template + empty mask (oracle sigdef/sigmask).
    // NOTE: USR1/USR2 are excluded — sibling tests install
    // on them concurrently (process-global dispositions).
    for (signo, flags) in [
        (SIGHUP, 0x42),
        (SIGTERM, 0x42),
        (SIGWINCH, 0x42),
        (SIGBUS, 0x40),
        (SIGSEGV, 0x40),
        (SIGINT, 0x0),
        (SIGCHLD, 0x0),
        (SIGALRM, 0x0),
        (SIGXCPU, 0x2),
        (SIGPROF, 0x2),
    ] {
        let mut old = DarwinSigaction {
            handler: 0xdead,
            mask: 0xdead,
            flags: 0xdead,
        };
        assert_eq!(
            mol_syscall::mol_sigaction(signo, core::ptr::null(), &mut old),
            0,
            "query signal {signo}"
        );
        assert_eq!(old.handler, SIG_DFL, "signal {signo}");
        assert_eq!(old.mask, 0, "signal {signo}");
        assert_eq!(old.flags, flags, "signal {signo}");
    }
}

#[test]
fn install_query_restore_roundtrip() {
    // Install IGN with a mask + flags on USR2.
    let mask = 1u32 << (SIGUSR1 - 1);
    let act = DarwinSigaction {
        handler: SIG_IGN,
        mask,
        flags: SA_RESTART | SA_NOCLDSTOP,
    };
    assert_eq!(
        mol_syscall::mol_sigaction(SIGUSR2, &act, core::ptr::null_mut()),
        0
    );
    // Query it back.
    let mut old = DarwinSigaction {
        handler: 0,
        mask: 0,
        flags: 0,
    };
    assert_eq!(
        mol_syscall::mol_sigaction(SIGUSR2, core::ptr::null(), &mut old),
        0
    );
    assert_eq!(old.handler, SIG_IGN);
    assert_eq!(old.mask, mask);
    assert_eq!(old.flags, SA_RESTART | SA_NOCLDSTOP);
    // Restore DFL; oldact reports the IGN we set.
    let dfl = DarwinSigaction {
        handler: SIG_DFL,
        mask: 0,
        flags: 0,
    };
    let mut prev = DarwinSigaction {
        handler: 0,
        mask: 0,
        flags: 0,
    };
    assert_eq!(mol_syscall::mol_sigaction(SIGUSR2, &dfl, &mut prev), 0);
    assert_eq!(prev.handler, SIG_IGN);
}

#[test]
fn delivery_sees_darwin_signo() {
    SEEN.store(-1, Ordering::SeqCst);
    let act = DarwinSigaction {
        handler: probe as *const () as u64,
        mask: 0,
        flags: 0,
    };
    assert_eq!(
        mol_syscall::mol_sigaction(SIGUSR1, &act, core::ptr::null_mut()),
        0
    );
    assert_eq!(mol_syscall::mol_kill(mol_syscall::mol_getpid(), SIGUSR1), 0);
    // Spin up to 5s for async delivery.
    let mut seen = -1;
    for _ in 0..500 {
        seen = SEEN.load(Ordering::SeqCst);
        if seen >= 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // The guest handler must see Darwin 30, not Linux 10.
    assert_eq!(seen, SIGUSR1);
    // Restore DFL.
    let dfl = DarwinSigaction {
        handler: SIG_DFL,
        mask: 0,
        flags: 0,
    };
    assert_eq!(
        mol_syscall::mol_sigaction(SIGUSR1, &dfl, core::ptr::null_mut()),
        0
    );
}

#[test]
fn invalid_installs_fail_mac_errno() {
    let act = DarwinSigaction {
        handler: probe as *const () as u64,
        mask: 0,
        flags: 0,
    };
    // KILL/STOP: neither install nor query.
    assert_eq!(
        mol_syscall::mol_sigaction(SIGKILL, &act, core::ptr::null_mut()),
        -1
    );
    common::assert_errno(EINVAL);
    let mut old = DarwinSigaction {
        handler: 0,
        mask: 0,
        flags: 0,
    };
    assert_eq!(
        mol_syscall::mol_sigaction(SIGSTOP, core::ptr::null(), &mut old),
        -1
    );
    common::assert_errno(EINVAL);
    // Out of range + SIGINFO (no Linux cousin).
    for signo in [0, 32, -1, SIGINFO] {
        assert_eq!(
            mol_syscall::mol_sigaction(signo, &act, core::ptr::null_mut()),
            -1,
            "signo {signo}"
        );
        common::assert_errno(EINVAL);
    }
    // SA_SIGINFO rejected (smoke boundary).
    let info = DarwinSigaction {
        handler: probe as *const () as u64,
        mask: 0,
        flags: SA_SIGINFO,
    };
    assert_eq!(
        mol_syscall::mol_sigaction(SIGUSR1, &info, core::ptr::null_mut()),
        -1
    );
    common::assert_errno(EINVAL);
    // Unknown flag bits rejected.
    let bad = DarwinSigaction {
        handler: SIG_IGN,
        mask: 0,
        flags: 0x0400,
    };
    assert_eq!(
        mol_syscall::mol_sigaction(SIGUSR1, &bad, core::ptr::null_mut()),
        -1
    );
    common::assert_errno(EINVAL);
    // kill: bad signo + probe semantics.
    assert_eq!(mol_syscall::mol_kill(mol_syscall::mol_getpid(), 0), 0);
    assert_eq!(mol_syscall::mol_kill(mol_syscall::mol_getpid(), 32), -1);
    common::assert_errno(EINVAL);
    assert_eq!(mol_syscall::mol_kill(1 << 30, 0), -1);
    common::assert_errno(ESRCH);
}
