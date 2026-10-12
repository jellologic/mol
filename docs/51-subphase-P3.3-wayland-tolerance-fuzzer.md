# 51 — Sub-phase P3.3: Wayland Protocols + Tolerance Metric + MIG Fuzzer

Parent: [33 — Phase P3](33-phase-P3-text-media.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: xdg-shell/decoration/fractional-scale/text-input;
SSIM-perceptual tolerance metric; MIG fuzzer lands in CI.
Milestone: protocol block interoperates with the weston reference
compositor; tolerance metric enforced; fuzzer running in CI.
Proven by: weston interop run + fuzzer CI leg.

Criteria:

- [ ] P3.3.1 Wayland xdg-shell + decoration + fractional-scale +
  text-input implemented, proven against weston (doc 09 M3).
- [ ] P3.3.2 GPU golden-image tolerance metric decided
  (SSIM/perceptual with allow-list, not raw per-pixel) and enforced
  in the suite — resolves the doc 16 G4 open item.
- [ ] P3.3.3 MACHFUZZER-class MIG fuzzer authored and running in CI.

Tests:

- [ ] P3.3.V1 hyperfine benches with regression thresholds wired for
  text-shape and blit paths (docs 13, 16).
