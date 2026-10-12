# 29 — Implementation Plan

Master build plan index. Phases P0–P6 plus recurring gate PG each
have their own doc (docs 30–37); every sub-phase has its own doc
(docs 38–63) with scope, measurable milestone, completion criteria
(`[ ]` to `[x]`), and validation. The milestone map below is the
progress tracker: a sub-phase is done only when its milestone proof
is green. Phase numbering tracks doc 09 milestones M0–M6
one-to-one; gates G1–G7 are doc 16; bring-up layers L0–L6 are
doc 12; fidelity tiers T0–T3 are doc 11.

## Goal

Build a ground-up, Rust-first macOS compatibility layer for Ubuntu
Desktop on ARM64 and AMD64, CLI-first then GUI, such that supported
apps function identically to Mac hardware per the doc 11 fidelity
contract, with every phase gated by executable checks and
Mac-oracle differential testing.

## Success Criteria

- Tier 1 CLI tools (macOS jq, curl, ffmpeg binaries) run on both
  arches with byte-identical stdout vs macOS (P1 exit).
- Tier 2 headless toolchain (macOS python3, git, node; Homebrew
  hello-world builds) runs on both arches (P2 exit).
- Tier 3 GUI apps (Calculator-class, TextEdit-class, one OSS Cocoa
  app) run scripted sessions unaided on both arches (P4 exit).
- Every phase exit gate (G1–G7) is green with zero new golden-trace
  failures vs baseline before the next phase starts.
- The dual-platform harness (doc 17) gates every merge with
  MATCH/DIVERGE/FLAKE verdicts; T0/T1/T2 failures block merge.
- Compat core stays dependency-minimal per the doc 23 decision
  matrix; every adopted dependency passes the doc 23 checklist.

## Approach

Reuse the phasing the research already normed instead of inventing a
new scheme: doc 09 owns the schedule (CLI-first, same-ISA before
cross-arch), doc 16 owns exit-criteria detail, doc 12 owns the
L0–L6 bring-up order inside P1–P2, doc 11 owns what "identical"
means, and docs 13/14/17–22 own how it is proven. P0 lands the repo,
legal posture, dependency policy, packaging, and the harness
skeleton first, so P1–P6 invoke a working gate rather than building
one mid-flight. The macOS trace collector (doc 18) is built in P0
because it needs no compat-layer code to start producing golden
traces. Key decisions:

- Phase = milestone: P0→M0 … P6→M6, plus recurring gate PG→G7 at
  every phase boundary. No parallel phases; init-order bugs are the
  costliest class (doc 12).
- Sub-phases run in order within a phase. A sub-phase is done only
  when its milestone proof is green (see Milestone map); the next
  sub-phase does not start on a red milestone.
- Handroll default per doc 23: Mac-observable behavior is handrolled
  behind trait seams; invisible infrastructure (crypto, Unicode,
  codecs, TLS) is adopted per doc 26; system libs link dynamically
  per doc 27. New dependencies pass the doc 23 seven-item checklist.
- Releases gate on libSystem-observable behavior (doc 11 boundary);
  syscall-level diffs are diagnostic signal with an allow-list.
- Compat tiers promote only on 3 consecutive green dashboards and
  demote on any release with a new golden diff or crash (doc 16).

Non-goals: UIKit/Catalyst (after P5 per doc 09); NKE/KPI network
filter parity (T3 best-effort, doc 11); snap packaging unless demanded
(doc 08 recommends deb + PPA primary); Windows hosts; AOT translation
before P6; a kernel module (doc 02 lesson 6: userspace only);
Apple-signed system apps needing Secure Enclave, SEP-backed keychain,
FairPlay DRM, or Gatekeeper enforcement; kexts; AVX-512 guests;
bit-perfect APFS semantics on day one (doc 01 v1 non-goals).

Grounding: this plan synthesizes docs 01, 02, 05, 06, 07, 08, 09,
11, 12, 13, 14, 15, 16, 17, 18, 20, 21, 23, 26, 27, and 28, all
inspected in this repo. Timelines below come only from doc 09 (P0:
weeks 1–4); later phases are sequenced, not dated.

## Phase index

| Phase | Doc | Sub-phases | Exit milestone |
|-------|-----|------------|----------------|
| P0 | [30 — Phase P0](30-phase-P0-foundation.md) | P0.1–P0.4 | Harness gates merges; first oracle trace in |
| P1 | [31 — Phase P1](31-phase-P1-cli-core.md) | P1.1–P1.4 | Tier 1 byte-identical on both arches |
| P2 | [32 — Phase P2](32-phase-P2-ipc-toolchain.md) | P2.1–P2.3 | Tier 2 headless; toolchain re-enters G1/G2 |
| P3 | [33 — Phase P3](33-phase-P3-text-media.md) | P3.1–P3.4 | Headless exit suite green on both arches |
| P4 | [34 — Phase P4](34-phase-P4-gui-shell.md) | P4.1–P4.3 | Tier 3 apps run unaided on both arches |
| P5 | [35 — Phase P5](35-phase-P5-metal.md) | P5.1–P5.3 | Metal game menu renders on both arches |
| P6 | [36 — Phase P6](36-phase-P6-cross-arch.md) | P6.1–P6.3 | Translated/native ratios published |
| PG | [37 — Phase PG](37-phase-PG-hardening.md) | PG.1–PG.2 | Release shippable with SBOM (every boundary) |

## Milestone map

One row per sub-phase, in build order. This table is the progress
tracker: tick a milestone only when its proof is green.

| Sub-phase | Milestone | Proven by |
|-----------|-----------|-----------|
| [P0.1](38-subphase-P0.1-legal-policy.md) | License + policy baseline recorded and enforced | deny/audit clean |
| [P0.2](39-subphase-P0.2-skeleton-ci.md) | Skeleton compiles on ARM64 + AMD64, locked + vendored | arch-matrix check green |
| [P0.3](40-subphase-P0.3-prefix-packaging.md) | `mol-loader --help` runs; empty prefix boots; debs install | boot test + offline deb install |
| [P0.4](41-subphase-P0.4-harness-traces.md) | Trivial test MATCHes; forced drift DIVERGE-blocks; first trace in | harness self-test green |
| [P1.1](42-subphase-P1.1-loader-dyld.md) | `LC_MAIN` reached, zero-diff load maps on thin+fat corpus | FileCheck load-map suite |
| [P1.2](43-subphase-P1.2-libsystem-syscalls.md) | Exact init order traced; smoke syscalls Mac-correct | init-order trace + conformance subset |
| [P1.3](44-subphase-P1.3-objc-swift.md) | `+load` ordering green; Swift dylibs load | ordering fixture + Swift load test |
| [P1.4](45-subphase-P1.4-server-tier1-exit.md) | Tier 1 runs byte-identical on both arches | corpus board + diff job |
| [P2.1](46-subphase-P2.1-mach-ipc-vm.md) | Mach IPC + VM semantics differential-clean | IPC differential + xnu subset |
| [P2.2](47-subphase-P2.2-xpc-launchd-gcd.md) | XPC/launchd lifecycle green under fault injection | fault-injection suite |
| [P2.3](48-subphase-P2.3-foundation-toolchain-exit.md) | Tier 2 headless on both arches; toolchain re-enters G1/G2 | Tier-2 board + re-entry run |
| [P3.1](49-subphase-P3.1-text-2d.md) | Text + 2D match references within tolerance | golden-image suite |
| [P3.2](50-subphase-P3.2-audio-video-web.md) | Headless audio/video/image-decode match oracle | headless render suite + snapshots |
| [P3.3](51-subphase-P3.3-wayland-tolerance-fuzzer.md) | Wayland block works; tolerance metric enforced; fuzzer in CI | protocol run + fuzzer leg |
| [P3.4](52-subphase-P3.4-headless-exit.md) | Headless exit suite passes on both arches | exit suite + zero parser crashers |
| [P4.1](53-subphase-P4.1-compositing-event-loop.md) | Windowed app renders frames and pumps events | driver launch + screenshot |
| [P4.2](54-subphase-P4.2-widgets-timing.md) | Panels scripted; timing tiers decided or T3 | panel scripts + AX-tree assertions |
| [P4.3](55-subphase-P4.3-tier3-exit.md) | Tier 3 apps run unaided on both arches | scripted sessions + sanitizers |
| [P5.1](56-subphase-P5.1-metal-api-matrix.md) | Metal samples render; family matrix published | golden frames |
| [P5.2](57-subphase-P5.2-msl-frontend.md) | MSL corpus compiles inside the T1 envelope | T1 comparators |
| [P5.3](58-subphase-P5.3-game-menu-exit.md) | Metal game menu renders on both arches | exit session + perf envelope |
| [P6.1](59-subphase-P6.1-matrix4.md) | Matrix #4 Tier 1 runs on AMD64 | check-tcg-analog corpus |
| [P6.2](60-subphase-P6.2-matrix3-tso.md) | Matrix #3 Tier 1 runs on ARM64 with TSO | LTP-style runs + self-host |
| [P6.3](61-subphase-P6.3-ratios-exit.md) | Translated/native ratios published | ratio report + no-escape dashboard |
| [PG.1](62-subphase-PG.1-hygiene-sanitizers-fuzz.md) | Binaries verify; suites sanitizer-clean; fuzzers quiet | make-check-analog + sanitizer leg |
| [PG.2](63-subphase-PG.2-benchmarks-release.md) | Benchmarks in budget; release shippable with SBOM | benchmark report + signed SBOM |

## Validation Plan

Each phase maps to its gates and suites; evidence is the gate
artifact, not a claim. The milestone map above is the fine-grained
tracker; this table is the phase-level rollup.

| Phase | Gates | Primary evidence |
|-------|-------|------------------|
| P0 | harness self-test, PG | JUnit MATCH on trivial test; debs install; deny/audit clean |
| P1 | G1, G2, PG | zero-diff load maps; byte-identical Tier-1 stdout; corpus board |
| P2 | G3, PG | XPC transcripts; launchd fault-injection green; toolchain re-entry |
| P3 | G4, PG | golden images within threshold; headless render logs |
| P4 | G5, PG | scripted-session AX-tree MATCH; unaided launch/exit |
| P5 | G6-part, PG | golden frames; family-matrix dashboard |
| P6 | G6, PG | per-arch dashboard; translated/native ratios |

Cross-cutting enforcement, all phases:

- Compare-job verdict per test-id: MATCH to pass, DIVERGE blocks
  merge until re-baselined with justification + sign-off, FLAKE
  quarantines on `(test, platform, renderer)` and never gates
  (doc 17).
- T0/T1/T2 failures block merge; T3 failures file issues only;
  the doc 11 divergence list lives in the normalizer, not per-test.
- `status.yaml` export registry tracks each doc 11 F-item's gate
  state per milestone (docs 11, 13).
- Tier promotion needs 3 consecutive green dashboards; any release
  with a new golden diff or crash demotes (doc 16).
- Highest-risk validation: Mach IPC differential fidelity (F2 T0,
  XL effort) and GUI event/runloop timing tiers (open in doc 16) —
  both can stall P2/P4 exits and need oracle traces before the code
  they gate is written.

## Risks / Open Questions

- License posture decided in P0.1: MIT OR Apache-2.0 (permissive).
  Consequence: Darling GPL-3.0 code cannot be reused and must be
  reimplemented (docs 08, 09).
- Metal API re-implementation is XL with no existing layer to fork
  (MoltenVK solves the inverse problem); the MSL→SPIR-V frontend is
  new (L pending spec review). P4 (GUI shell) and P5 are the two
  highest-effort phases; doc 01 warns AppKit/GUI work dominates
  the schedule (docs 01, 05, 09).
- libxpc internals are unpublished — bring-up is specified at the
  launchd/XPC API level only; `_libxpc_initializer` bootstrap order
  is reverse-engineered from behavior (doc 12 L5).
- `NSApplicationMain` internals are closed-source; P4 depends on
  golden-trace-derived behavior specs (doc 12 L6).
- Intel-Mac oracle availability vs Rosetta sunset affects P6
  oracle choice (docs 06, 15).
- Benchmark gates (3%/10%) vs TSO variance (~9%) + Accelerate
  per-CPU / GCD auto-split nondeterminism need the pinned oracle
  config + variance budget before they bind (doc 16, PG.2).
- GPU golden-image tolerance metric and audio latency tier are
  open until P3.3; NSEvent/runloop/vsync tiers open until P4.2.
- Guest sandbox enforcement (F13 T2) enters scope at P4 with App
  Sandbox; P1–P3 guests run unsandboxed as a documented,
  status.yaml-recorded scoping decision (docs 11, 12).
- F12 (NKE/KPI network filters) stays T3 tracked-only with no
  milestone (doc 11).
- TLS/SecureTransport compatibility lands in P3.2, but its fidelity
  surface is missing from the doc 11 F-table (OQ-117); P1–P2 exits
  are explicitly non-TLS.
- Full backlog: doc 10, OQ-1 through OQ-119.
