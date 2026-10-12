# 42 — Sub-phase P1.1: Mach-O Loader + dyld Engine

Parent: [31 — Phase P1](31-phase-P1-cli-core.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: parse, map, fixups, init order, closures; guest codesign
policy; Tier-1 linkage closure; arm64e handling.
Milestone: `LC_MAIN` reached on the thin+fat corpus with zero-diff
load maps.
Proven by: FileCheck golden load-map suite.

Criteria:

- [x] P1.1.1 Loader parses thin+fat Mach-O incl `LC_LOAD_DYLINKER`,
  maps segments, applies rebase/bind opcodes, transfers to `LC_MAIN`
  with the Darwin stack layout (doc 16 G1).
- [x] P1.1.2 dyld engine runs loadDependents → applyFixups →
  runAllInitializersForMain; closure fast-path + override/reboot
  invalidation follow policy; one cache mode recorded per run
  (doc 12 L1/L3).
- [x] P1.1.3 Guest codesign policy enforced as documented: parse
  `LC_CODE_SIGNATURE` blobs, skip verification, honor gating
  entitlements (JIT, library validation) (doc 03).
- [x] P1.1.4 Shared-cache strategy decided (parse the cache vs ship
  prebound dylibs); Tier-1 linkage closure audited (`otool -L`) —
  every dependent dylib resolves real-or-stub in the prefix
  (docs 03, 09 M1).
- [x] P1.1.5 arm64e slices handled per matrix-#1 policy: strip PAC or
  execute as NOPs on non-PAC Linux ARM64; key-compat audit where
  PAC hardware exists (doc 06).

Tests:

- [x] P1.1.V1 Load maps zero-diff vs `otool -l`/`dyld_info` on
  thin/fat/PIE corpus via FileCheck golden traces (doc 16 G1).
- [x] P1.1.V2 AFL++ on the Mach-O parser from the first parser
  commit; libFuzzer on targeted parse functions (doc 13).

Exit evidence:

- P1.1.1: `mol-macho` (thin+fat parse, load commands, rebase/bind
  opcodes, chained fixups incl formats 1/2/6/9/12, exports trie,
  codesign blobs, `LC_UUID=0x1b` per SDK headers) + `mol-loader run`
  (mmap at `vmaddr+slide`, fixup application, `mprotect`, Darwin
  stack, asm transfer). Proven by 6/6 guest-execution tests on
  Linux arm64 AND amd64: exit codes, init-before-entry marker,
  argc echo, clean cross-arch refusal.
- P1.1.2: `mol-dyld` (BFS `loadDependents`, `@rpath`/`@loader_path`/
  `DYLD_*` search, fixup planning with two-level/flat/weak
  resolution, post-order initializers, closure decision, per-run
  `RunRecord`). 20 engine unit tests on synthetic images.
- P1.1.3: `edge_allowed` library-validation matrix + `jit_allowed`
  entitlement check, both unit-tested; enforcement runs on every
  linked edge in `run_guest`. Unsigned prefix system libs count as
  platform (mol's own ABI surface); hardened-runtime guests need an
  allow-JIT entitlement for future JIT mappings.
- P1.1.4: decision — ship prebound dylibs as real files under the
  prefix (`/usr/lib/*.dylib`, pre-macOS-11 layout); do NOT parse
  Apple's shared cache (nothing in it is redistributable, and our
  libSystem is a clean reimplementation). Revisit a mol-built cache
  file only if Tier-1 launch times demand it. Closure audit
  (Oct 2026, Apple Silicon): jq → libSystem.B only; curl →
  libcurl.4 + libz.1 + libSystem.B; brew ffmpeg → 7 libav* +
  16 Apple frameworks + libz/libbz2/libiconv + 9 portable brew
  libs. Portable libs (libcurl, libav*, openssl, codecs) will be
  built from upstream source for the prefix; Apple frameworks get
  loadable stubs in P1.4 and real implementations in P2–P5.
  `/usr/lib/*.dylib` no longer exist as files on macOS (cache
  only), so transitive cache-only deps are unauditable from the
  host and resolve in our prefix by construction.
- P1.1.5: authenticated fixups (formats 1/9/12) parse and resolve
  to plain targets with `stripped_pac` recorded; `pac_available`
  (via `is_aarch64_feature_detected!("paca")`) joins the run
  record for the key-compat audit. PAC instructions in guest code
  execute natively (NOPs on non-PAC ARM64 by hardware design).
  Load-bearing: system jq/curl ship x86_64 + arm64e + arm64e.x1
  slices with NO plain arm64 slice, so Tier-1 on ARM64 exercises
  the arm64e path (OQ-120 tracks the `.x1`/subtype-12 question).
- P1.1.V1: 12 oracle differentials (`otool -l`, `dyld_info
  -fixups -exports`, `codesign -dvvv`) green on macOS plus 12
  byte-exact goldens enforced on Linux CI. "FileCheck golden
  traces" is served by the handrolled exact-match golden suite
  per the dependency philosophy (no LLVM checkout in CI).
- P1.1.V2: `tools/fuzz-afl.sh` (AFL++ 150s clean; found and fixed
  a real iterator-fuse hang first) and `tools/fuzz-libfuzzer.sh`
  (4 cargo-fuzz targets × 60s clean), both gated in the CI `fuzz`
  job from this commit forward.
