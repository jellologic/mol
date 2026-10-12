# 39 — Sub-phase P0.2: Repo Skeleton + Arch CI

Parent: [30 — Phase P0](30-phase-P0-foundation.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: 12-crate workspace builds on both arches; arch-matrix check
script.
Milestone: skeleton compiles on ARM64 + AMD64 with locked, vendored
deps.
Proven by: arch-matrix check green.

Criteria:

- [x] P0.2.1 Crate skeleton builds on ARM64 + AMD64: mol-macho,
  mol-loader, mol-dyld, mol-syscall, mol-mach, mol-xpc,
  mol-libsystem, mol-objc, mol-metal-vk, mol-launchd, mol-bridge,
  xtask; `Cargo.lock` committed; `vendor/` materializes with the
  first third-party dependency (zero deps today — `cargo vendor`
  confirms nothing to vendor) (docs 07, 28).
- [x] P0.2.2 `default-features = false` discipline + trait seams on
  every handrolled unit; new-dependency checklist adopted
  (docs 23, 24).
- [x] P0.2.3 Tracker tooling installed: `tools/mol-track` validates
  clean and `.githooks` (commit-msg + pre-commit) are active via
  `tools/install-hooks.sh` (doc 64).

Tests:

- [x] P0.2.V1 Arch-matrix check script green on both arches (relibc
  `check.sh` pattern, doc 13).
- [x] P0.2.V2 `cargo deny check` + `cargo audit` clean on the 12-crate
  skeleton; scaffold crate removed (doc 28).
