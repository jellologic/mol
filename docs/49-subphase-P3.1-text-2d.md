# 49 — Sub-phase P3.1: Text + 2D Graphics

Parent: [33 — Phase P3](33-phase-P3-text-media.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: CoreText over HarfBuzz/FreeType/Fontconfig; CoreGraphics-2D
over Cairo/Skia.
Milestone: shaping and compositing match references within
tolerance.
Proven by: golden-image suite.

Criteria:

- [ ] P3.1.1 CoreText shapes ASCII+CJK+emoji, RTL/LTR, matching
  reference within tolerance (doc 16 G4).
- [ ] P3.1.2 CoreGraphics paths/gradients/images/compositing golden
  diffs pass (doc 16 G4).

Tests:

- [ ] P3.1.V1 Golden-image corpus with per-suite threshold +
  FileCheck stderr traces (doc 16 G4).
