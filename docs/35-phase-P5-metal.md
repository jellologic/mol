# 35 — Phase P5: Metal + MSL over Vulkan (→M5, gate G6-part)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: P4 exit milestone green (Tier 3 unaided).

Objective: Metal API re-implementation with a new MSL→SPIR-V
frontend; UIKit/Catalyst explicitly out of scope.

Requirements: Metal API over `ash` core behind the adopted-dep
seam (XL overall, doc 01); family feature-parity matrix as the
tracking artifact; new MSL→SPIR-V frontend (L pending spec review,
doc 05 — no blessed tool exists, MoltenVK solves the inverse).

Sub-phases (in order):

- [P5.1 — Metal API + family matrix](56-subphase-P5.1-metal-api-matrix.md):
  Metal samples render; family matrix published.
- [P5.2 — MSL→SPIR-V frontend](57-subphase-P5.2-msl-frontend.md):
  MSL corpus compiles inside the T1 envelope.
- [P5.3 — Exit: game menu renders](58-subphase-P5.3-game-menu-exit.md):
  Metal game menu renders on both arches.

Exit milestone: one simple Metal game's menu renders on both
arches (P5.3 green). Proven by: exit session + perf envelope
within budget.
