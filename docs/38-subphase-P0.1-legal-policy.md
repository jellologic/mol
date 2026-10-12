# 38 — Sub-phase P0.1: Legal + Policy Baseline

Parent: [30 — Phase P0](30-phase-P0-foundation.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: license posture decided and recorded; dependency policy live
in CI.
Milestone: license + policy baseline recorded and enforced.
Proven by: deny/audit clean on the skeleton tree.

Criteria:

- [x] P0.1.1 License decision (permissive vs GPL-3.0) recorded with
  rationale; SPDX headers in tree (docs 08, 28).
- [x] P0.1.2 `cargo deny` license allow-list + sources check
  configured and passing on P0.1 scaffolding (empty member crate;
  `cargo deny` needs a workspace member); re-proven on the real
  skeleton in P0.2 (docs 23, 28).

Tests:

- [x] P0.1.V1 `cargo deny check` + `cargo audit` clean in CI
  (doc 28).
