# 32 — Phase P2: IPC Depth + Headless Toolchain (→M2, gate G3, layers L5–L6)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: P1 exit milestone green (Tier 1 byte-identical).

Objective: Mach IPC, XPC/launchd, GCD, and Foundation carry a
headless dev toolchain on both arches.

Requirements: full Mach IPC (ports, rights, messages, port sets,
OOL); MIG task/thread/exception ports; VM pager/COW nuance;
Tier-2 linkage closure audit; XPC request/reply with
crash-mid-reply semantics; launchd socket activation with
on-demand/crash-restart/idle-kill; notifyd; libdispatch Linux port;
CFRunLoop on epoll; `__CFInitialize` + usable Foundation; ICU +
fs fidelity; IOKit stubs only as toolchain needs demand.

Sequencing note: P2.1 tackles Mach IPC first although doc 04 ranks
it hardest — XPC, exceptions, and launchd all depend on ports, so
dependency order wins over difficulty order.

Sub-phases (in order):

- [P2.1 — Mach IPC + VM semantics](46-subphase-P2.1-mach-ipc-vm.md):
  Mach IPC + VM semantics differential-clean.
- [P2.2 — XPC + launchd + GCD](47-subphase-P2.2-xpc-launchd-gcd.md):
  XPC/launchd lifecycle green under fault injection.
- [P2.3 — Foundation + toolchain exit](48-subphase-P2.3-foundation-toolchain-exit.md):
  Tier 2 headless on both arches; toolchain re-enters G1/G2.

Exit milestone: macOS python3, git, node run headless on both
arches over local/non-TLS protocols; Homebrew formulae build
hello-world-class packages; toolchain output re-enters G1/G2 gates
clean (P2.3 green, Tier 2). Proven by: Tier-2 board + toolchain
re-entry run.
