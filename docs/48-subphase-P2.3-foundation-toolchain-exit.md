# 48 — Sub-phase P2.3: Foundation + Toolchain Exit

Parent: [32 — Phase P2](32-phase-P2-ipc-toolchain.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: `__CFInitialize` + Foundation; headless toolchain;
MIG-fuzzer design frozen; ICU + fs fidelity.
Milestone: macOS python3/git/node headless on both arches over
local protocols; toolchain output re-enters G1/G2.
Proven by: Tier-2 board + toolchain re-entry run.

Criteria:

- [ ] P2.3.1 Foundation usable by Tier 1 CLI (CF classes registered,
  allocator-first order); CLI tools need no AppKit (doc 12 L6).
- [ ] P2.3.2 Exit: macOS python3, git, node run headless on both
  arches over local/non-TLS protocols (git local clone/commit/
  status; node local http + fs); Homebrew formulae build
  hello-world-class packages; toolchain (clang/cc1 + ld +
  `codesign -s -`) output re-enters G1/G2 gates clean; TLS upgrade
  in P3.2 (docs 09 M2 Tier 2, 16 G3 DoD).
- [ ] P2.3.3 ICU pinned to the oracle version; collation/case/
  normalization/dates differential-clean (F5 T0); F7–F10
  filesystem-fidelity suites green per tier (doc 11).

Tests:

- [ ] P2.3.V1 MACHFUZZER-class MIG fuzzer design frozen;
  implementation scheduled in P3 (doc 13 XL item).
- [ ] P2.3.V2 Tier-2 workload smoke scripts byte-identical vs macOS:
  python json + local http, git local clone/commit, node local
  http + fs (doc 16 G3).
- [ ] P2.3.V3 `status.yaml` updated for P2-owned F-items
  (F2, F5, F7–F10) (docs 11, 13).
