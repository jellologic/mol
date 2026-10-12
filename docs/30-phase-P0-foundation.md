# 30 — Phase P0: Skeleton, Legal, CI, Packaging, Harness Seed (→M0)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: none (first phase).

Objective: a buildable, gated, shippable empty vessel plus the first
oracle traces.

Requirements: license posture decided and recorded; 12-crate
skeleton per doc 07 with `default-features = false` discipline;
trait seams on every handrolled unit; prefix boots PID1 on both
arches; harness skeleton runs one trivial test through both legs;
PPA pipeline builds offline from vendored crates.

Sub-phases (in order):

- [P0.1 — Legal + policy baseline](38-subphase-P0.1-legal-policy.md):
  license + policy baseline recorded and enforced.
- [P0.2 — Repo skeleton + arch CI](39-subphase-P0.2-skeleton-ci.md):
  skeleton compiles on ARM64 + AMD64, locked + vendored.
- [P0.3 — Prefix boot + packaging](40-subphase-P0.3-prefix-packaging.md):
  `mol-loader --help` runs; empty prefix boots; debs install.
- [P0.4 — Harness seed + first traces + index](41-subphase-P0.4-harness-traces.md):
  trivial test MATCHes; forced drift DIVERGE-blocks; first trace in.

Exit milestone: the harness gates merges and the first oracle trace
is checked in (P0.4 green). Proven by: harness self-test green.
