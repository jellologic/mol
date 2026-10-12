# 63 — Sub-phase PG.2: Benchmarks + Release Artifact

Parent: [37 — Phase PG](37-phase-PG-hardening.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: benchmark gates, SBOM, sign-off discipline.
Milestone: benchmarks within budget (once the oracle config lands);
release shippable with SBOM.
Proven by: benchmark report + signed SBOM + clean dashboard.

Criteria:

- [ ] PG.2.1 Release ships with SBOM + signed artifacts + security
  log review (doc 16 G7 DoD).
- [ ] PG.2.2 Benchmark gates: SingleSource+MultiSource correctness
  100% with matching reference hashes; microbenches (loader
  startup, IPC round-trip, text-shape, blit) show no >3% regression;
  app-level Tier-1 wall-time and Tier-3 fps/frame-time p95 recorded;
  cross-arch ratios flag any >10% slip in release notes (doc 16).
  Blocked until the pinned-oracle-config + variance budget lands —
  resolves the doc 16 open tension with TSO variance and GCD
  auto-split nondeterminism.

Tests:

- [ ] PG.2.V1 Dashboard shows zero new failures vs baseline; any
  golden diff re-baselined only with written justification +
  reviewer sign-off (doc 16 regression policy).
