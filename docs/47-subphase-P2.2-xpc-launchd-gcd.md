# 47 — Sub-phase P2.2: XPC + launchd + GCD

Parent: [32 — Phase P2](32-phase-P2-ipc-toolchain.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: XPC request/reply, launchd lifecycle, notifyd, libdispatch
port, CFRunLoop on epoll, full init-order proof.
Milestone: on-demand / crash-restart / idle-kill cycle green under
fault injection.
Proven by: lifecycle fault-injection suite.

Criteria:

- [ ] P2.2.1 XPC request/reply incl invalid-connection-on-crash
  (doc 16 G3).
- [ ] P2.2.2 Main queue + a MachService listener live; on-demand
  launch / crash-restart / idle-kill cycle green (doc 12 L5).

Tests:

- [ ] P2.2.V1 XPC conformance suite asserting privilege-separation
  and sandbox attrs (doc 16 G3).
- [ ] P2.2.V2 launchd lifecycle fault-injection suite (doc 16 G3).
- [ ] P2.2.V3 notifyd pub/sub tokens + state values
  differential-clean (doc 04).
- [ ] P2.2.V4 Full libSystem init order incl real dispatch/XPC
  verified by traced launch — completes P1.2.1 (doc 12 L2).
