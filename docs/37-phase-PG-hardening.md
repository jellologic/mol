# 37 — Phase PG: Release Hardening Gate (→G7, every phase boundary)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: the gated phase (P0–P6) is code-complete; runs at every
phase exit, not just at the end.

Objective: no phase is done until the release bar holds.

Sub-phases (in order):

- [PG.1 — Binary hygiene + sanitizers + fuzz](62-subphase-PG.1-hygiene-sanitizers-fuzz.md):
  binaries verify; suites sanitizer-clean; fuzzers quiet.
- [PG.2 — Benchmarks + release artifact](63-subphase-PG.2-benchmarks-release.md):
  benchmarks in budget; release shippable with SBOM.

Exit milestone: release shippable with SBOM + signed artifacts and
benchmarks within budget (PG.2 green). Proven by: benchmark report
+ signed SBOM + clean dashboard.

## P0 exit applicability

At the P0 exit there are no shippable artifacts, so the artifact-bound
proofs are vacuous: PG.1.1 (codesign), PG.1.3 (fuzz), PG.2.1 (SBOM),
PG.2.2 (benchmarks) bind from P1, when the loader ships behavior.
The applicable subset — policy clean (`cargo deny` + `cargo audit`),
arch matrix green, tracker valid — held at P0 exit. PG boxes stay
unticked until the first artifact-bearing phase.
