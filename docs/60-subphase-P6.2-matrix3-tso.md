# 60 — Sub-phase P6.2: Matrix #3 (x86_64 → ARM64) + TSO

Parent: [36 — Phase P6](36-phase-P6-cross-arch.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: IR JIT/dynarec; TSO emulation last; translator checklist
incl 16K/4K page-size handling (doc 16 G6 open); PAC and
Rosetta-parity reporting (docs 06, 15).
Milestone: x86_64-macOS Tier 1 binaries run on ARM64 Linux with
TSO emulation.
Proven by: LTP-style runs + self-host clean.

Criteria:

- [ ] P6.2.1 Matrix #3: x86_64-macOS Tier 1 binaries run on ARM64
  Linux with TSO emulation (doc 09 M6).
- [ ] P6.2.2 Syscall translator covers endian + 32/64-bit size adjust
  + ioctl adjust + page-size mapping (doc 16 G6).

Tests:

- [ ] P6.2.V1 Per-arch LTP-style syscall runs; self-hosting test;
  Universal/fat `codesign -v --all-architectures` (doc 16 G6).
- [ ] P6.2.V2 Functional tests as Python unittest+TAP
  (`make check-functional` pattern, doc 16 G6).
