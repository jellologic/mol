# 34 — Phase P4: GUI Shell, First Pixels (→M4, gate G5, Tier 3)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: P3 exit milestone green (headless suite passes).

Objective: minimal AppKit over the P3 foundations; ruthlessly
scoped per Darling's ten-year hello-world precedent (docs 02, 09).

Requirements: CoreAnimation layer tree → Vulkan scene graph /
Wayland subsurfaces; AppKit event loop + responder chain + minimal
widget set; `NSApplicationMain` sequence specified from behavior +
golden traces (closed source, doc 12 L6); menu bar, alerts,
file-open panel, drag-drop paths, pasteboard/UTI; Cocoa Drawing
chapters smoke; exit targets named and pinned.

Sub-phases (in order):

- [P4.1 — Compositing + event loop](53-subphase-P4.1-compositing-event-loop.md):
  windowed app renders frames and pumps events.
- [P4.2 — Widgets + panels + timing tiers](54-subphase-P4.2-widgets-timing.md):
  panels scripted; timing tiers decided or T3.
- [P4.3 — Tier-3 exit apps](55-subphase-P4.3-tier3-exit.md):
  Tier 3 apps run unaided on both arches.

Exit milestone: Calculator-clone and TextEdit-clone (authored test
apps) usable; one OSS Cocoa app runs; all on both arches (P4.3
green, Tier 3). Proven by: unaided scripted sessions +
sanitizer-clean suites.
