# 54 — Sub-phase P4.2: Widgets + Panels + Timing Tiers

Parent: [34 — Phase P4](34-phase-P4-gui-shell.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: menu bar, alerts, file-open panel, drag-drop paths, Cocoa
Drawing smoke; NSEvent/runloop/vsync tier decision; pasteboard/UTI.
Milestone: panels scripted end-to-end; timing tiers decided or
explicitly T3.
Proven by: panel scripts + AX-tree assertions.

Criteria:

- [ ] P4.2.1 Menu bar, alerts, file-open panel, drag-drop paths work
  (doc 16 G5).
- [ ] P4.2.2 NSEvent-timestamp / runloop-ordering / vsync timing
  fidelity tier decided or explicitly carried as T3 — resolves the
  doc 16 G5 open item.
- [ ] P4.2.3 Pasteboard/UTI differential-clean: multiple
  representations per item, UTI conformance (F15 T0-bytes/T2,
  doc 11).

Tests:

- [ ] P4.2.V1 Accessibility-tree comparison (AX vs AT-SPI) as the
  primary stable surface; pixels only vs same-platform baselines
  (doc 21).
