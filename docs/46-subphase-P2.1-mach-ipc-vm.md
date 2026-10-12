# 46 — Sub-phase P2.1: Mach IPC + VM Semantics

Parent: [32 — Phase P2](32-phase-P2-ipc-toolchain.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: ports/rights/messages/sets/OOL, MIG task/thread/exception
ports, VM pager/COW nuance, Tier-2 linkage closure.
Milestone: Mach msg-id → reply semantics differential-clean;
VM nuance matches oracle behavior.
Proven by: IPC differential suite + xnu semantic subset.

Criteria:

- [ ] P2.1.1 Mach IPC differential-clean vs oracle on msg-id → reply
  semantics (docs 11 F2 T0, 16 G3).
- [ ] P2.1.2 VM pager/COW nuance matches oracle behavior (doc 16 G3).
- [ ] P2.1.3 Tier-2 linkage closure audited; codesign/Security
  service dependencies mapped (informs TLS + sandbox scope)
  (docs 09 M2, 16 G3).

Tests:

- [ ] P2.1.V1 darwin-xnu tests subset (coalition, ports, bpf) adapted
  as semantic-drift suite (doc 13).
- [ ] P2.1.V2 Record/replay attached to failing IPC cases only
  (doc 13).
