# 42 — Sub-phase P1.1: Mach-O Loader + dyld Engine

Parent: [31 — Phase P1](31-phase-P1-cli-core.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: parse, map, fixups, init order, closures; guest codesign
policy; Tier-1 linkage closure; arm64e handling.
Milestone: `LC_MAIN` reached on the thin+fat corpus with zero-diff
load maps.
Proven by: FileCheck golden load-map suite.

Criteria:

- [ ] P1.1.1 Loader parses thin+fat Mach-O incl `LC_LOAD_DYLINKER`,
  maps segments, applies rebase/bind opcodes, transfers to `LC_MAIN`
  with the Darwin stack layout (doc 16 G1).
- [ ] P1.1.2 dyld engine runs loadDependents → applyFixups →
  runAllInitializersForMain; closure fast-path + override/reboot
  invalidation follow policy; one cache mode recorded per run
  (doc 12 L1/L3).
- [ ] P1.1.3 Guest codesign policy enforced as documented: parse
  `LC_CODE_SIGNATURE` blobs, skip verification, honor gating
  entitlements (JIT, library validation) (doc 03).
- [ ] P1.1.4 Shared-cache strategy decided (parse the cache vs ship
  prebound dylibs); Tier-1 linkage closure audited (`otool -L`) —
  every dependent dylib resolves real-or-stub in the prefix
  (docs 03, 09 M1).
- [ ] P1.1.5 arm64e slices handled per matrix-#1 policy: strip PAC or
  execute as NOPs on non-PAC Linux ARM64; key-compat audit where
  PAC hardware exists (doc 06).

Tests:

- [ ] P1.1.V1 Load maps zero-diff vs `otool -l`/`dyld_info` on
  thin/fat/PIE corpus via FileCheck golden traces (doc 16 G1).
- [ ] P1.1.V2 AFL++ on the Mach-O parser from the first parser
  commit; libFuzzer on targeted parse functions (doc 13).
