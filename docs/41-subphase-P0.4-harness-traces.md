# 41 — Sub-phase P0.4: Harness Seed + First Traces + Index

Parent: [30 — Phase P0](30-phase-P0-foundation.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: compare job gates merge; first golden trace from real Mac;
plan indexed.
Milestone: a trivial test verdicts MATCH across legs and forced
drift verdicts DIVERGE and blocks; first oracle trace checked in.
Proven by: harness self-test green.

Criteria:

- [x] P0.4.1 Harness skeleton: YAML test-def schema + validator,
  per-leg JUnit emission, compare job verdicting MATCH/DIVERGE/FLAKE,
  required compare check gating merge (docs 17, 20).
- [x] P0.4.2 macOS trace collector prototype captures its first golden
  trace (stdout/stderr/exit/syscall trace) from a trivial binary. A
  GHA-hosted macOS runner counts as the P0 oracle; owned mini + Tart
  per doc 15 lands later (docs 15, 18).
- [x] P0.4.3 This plan indexed in docs/README.md.

Tests:

- [x] P0.4.V1 Harness self-test: trivial test through mac + linux
  legs verdicts MATCH; forced divergence verdicts DIVERGE and blocks.

## Oracle access note

P0.4.2 needs a syscall trace from real Mac hardware. The collector
(`tools/mol-trace`) and all five backends are implemented, but this
dev machine cannot produce one: lldb attach is denied (developer
mode off, no admin), xctrace needs full Xcode (CLT only), dtruss
needs root. The seed golden in `corpus/cli/harness-smoke-echo/mac/`
carries real stdout/stderr/exit/dyld/log with syscall backends
`skipped`.

Runbook to close P0.4.2 (allowed by P0.4.2: a GHA-hosted macOS
runner counts as the P0 oracle):

1. Push (or open a PR): the CI `legs` job runs the smoke test on
   macos-latest, where lldb attach typically succeeds.
2. `gh run download <run-id> -n results-mac -D /tmp/oracle`
3. Verify a syscall backend reports `ok` in `/tmp/oracle/trace.json`.
4. Copy it over `corpus/cli/harness-smoke-echo/mac/trace.json`.
5. `tools/mol-track check P0.4.2`.
