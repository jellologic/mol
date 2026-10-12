# 36 — Phase P6: Cross-Arch Translation (→M6, gate G6)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: P5 exit milestone green (game menu renders).

Objective: translated pairs run Tier 1 after both same-ISA legs
are solid. Matrix #4 (arm64-macOS on AMD64) before #3
(x86_64-macOS on ARM64) per doc 09.

Requirements: QEMU-TCG-class translator + mol loader for matrix #4;
IR JIT/dynarec for matrix #3; TSO emulation last; 16K/4K page-size
handling in the translator checklist (doc 16 G6 open); PAC and
Rosetta-parity reporting (docs 06, 15).

Sub-phases (in order):

- [P6.1 — Matrix #4 (arm64 → AMD64)](59-subphase-P6.1-matrix4.md):
  Matrix #4 Tier 1 runs on AMD64.
- [P6.2 — Matrix #3 (x86_64 → ARM64) + TSO](60-subphase-P6.2-matrix3-tso.md):
  Matrix #3 Tier 1 runs on ARM64 with TSO.
- [P6.3 — Ratios published + exit](61-subphase-P6.3-ratios-exit.md):
  translated/native ratios published.

Exit milestone: translated-vs-native perf ratios published per
benchmark (P6.3 green). Proven by: ratio report + no-escape
dashboard.
