# 53 — Sub-phase P4.1: Compositing + Event Loop

Parent: [34 — Phase P4](34-phase-P4-gui-shell.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: CoreAnimation layer tree → Vulkan scene graph / Wayland
subsurfaces; event loop + window + views + responder chain; exit
targets named.
Milestone: a windowed app renders first frames and pumps events.
Proven by: driver launch + screenshot on the Calculator-class app.

Criteria:

- [ ] P4.1.1 CoreAnimation layer tree → Vulkan scene graph / Wayland
  subsurfaces (doc 09 M4).
- [ ] P4.1.2 Event loop + window + view hierarchy + first-responder
  chain live (doc 16 G5).
- [ ] P4.1.3 Exit targets named and pinned: OSS Cocoa app selected;
  Calculator-clone + TextEdit-clone authored-test-app specs frozen
  (genuine Apple-signed apps out of scope per doc 01).

Tests:

- [ ] P4.1.V1 Scripted GUI driver launches the Calculator-class app,
  captures screenshots, injects synthetic events (doc 16 G5).
