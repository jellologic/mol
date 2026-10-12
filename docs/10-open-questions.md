# 10 — Open Questions

Preserved gaps from prior research. Each item names what is missing and what
unblocks it. Numbered OQ-1… for roadmap tracking.

## Loader / dyld / signing

- OQ-1 `dyld_cache_format.h` exact current path unfetched (cache claims rest
  on CacheLayout.md + source listing). Unblocks: shared-cache parse decision.
- OQ-2 Parse-shared-cache vs supply-prebound-dylibs: no result picks one.
  Unblocks: M1 dyld scope.
- OQ-3 AMFI internals rest on a secondary writeup + `CS_*` headers (no
  Apple-published source). Unblocks: nothing critical (parse-only posture stands).
- OQ-4 opensource.apple.com direct pages 404 to fetchers; evidence came from
  apple-oss-distributions mirrors. Unblocks: source-pinning hygiene.
- OQ-5 Notarization/Gatekeeper bypass needs on Linux: r4 says skip-verify +
  parse `LC_CODE_SIGNATURE`, but no ticket/staple/offline-bypass design.
- OQ-6 `object` crate is trusted-inputs-only vs adhoc/unsigned loader threat
  model (unresolved contradiction). Unblocks: hostile-binary policy for M1.

## Kernel ABI / IPC

- OQ-7 `CFRunLoop.c` full source body 404; only CFMachPort integration API
  confirmed.
- OQ-8 XPC wire format / libxpc private protocol: no open primary source
  inspected (framework docs only). mol-xpc wire design currently sourceless.
- OQ-9 BSD syscall class prefix `0x2000000` on x86-64: secondary snippets
  only, no inspected primary.
- OQ-10 `mach_msg_trap` man page + KernelProgramming IPC chapter both 404.
- OQ-11 launchd plist/job semantics uninspected; AF_UNIX+socket-activation
  proposal vs bootstrap-registry proposal unreconciled.
- OQ-12 Seatbelt sandbox-profile compiler + entitlement/TCC enforcement
  model: r4 says ignore entitlements except JIT/library-validation; no
  profile compiler, sandboxd, or TCC mapping.
- OQ-13 UID/GID + container/home mapping: no macOS users/groups
  (_appstore/mobile/staff) → Linux design, ownership, setuid, per-app
  container dirs.
- OQ-14 Swift runtime: r4 requires stable-ABI Swift dylibs, but the crate
  layout has no Swift runtime crate.
- OQ-15 ObjC: category-attach + `+load` superclass-first order required, but
  no proof objc2 re-export reproduces it.

## Filesystems

- OQ-16 APFS/case-insensitive behavior: zero coverage — NFD/NFC
  normalization, case-folding, resource forks/xattrs, clone/sparse, firmlinks.

## GUI / media

- OQ-17 MSL language spec body + MSL→SPIR-V frontend scope (spec PDF
  uninspected; effort-L rating ungrounded; no grammar/capture design).
- OQ-18 Metal family feature-parity matrix missing (arg buffers, mesh,
  raytracing, tile); MoltenVK/gfx-portability only as inverse reference.
- OQ-19 Metal-on-Vulkan backend maturity: README-level mention only; mapping
  coverage unverified.
- OQ-20 X11 ICCCM/EWMH specifics uninspected (Wayland side only).
- OQ-21 Modern developer.apple.com API bodies are JS-gated; archive guides
  used instead.
- OQ-22 GUI schedule tension: Tier 3 Calculator/TextEdit vs Darling's 10-year
  hello-world-only GUI precedent.

## Cross-arch

- OQ-23 ARM64 vs x86_64 support status + perf uninspected; distro survey of
  default ARM64 page size missing (4K assumed; kernel docs confirm granule is
  a build choice incl. 64KB).
- OQ-24 Whether x86_64 macOS binaries assume 4K mprotect granularity (cf.
  Rosetta-4K study).
- OQ-25 No official Apple M1-TSO documentation (evidence: XNU
  `ACTLR_EL1_EnTSO` + FEX measurements).
- OQ-26 WineHQ wiki JS-walled; Wine findings rest on inspected source, not
  the Developer Guide prose.

## Legal / distribution

- OQ-27 SDK-header redistribution grant unknown; stub-gen-on-Mac (SDK +
  class-dump) contradicts the Xcode redistribution ban — no legal path named.
- OQ-28 EU Software Directive Art. 6 interoperability exception not researched.
- OQ-29 GPL-3.0 whole-work copyleft vs permissive Rust crates/relibc model:
  reuse conflict unnamed.
- OQ-30 Launchpad PPA per-release build matrix + moderation rules uninspected
  (login-gated); dput source-upload + offline cargo-vendor flow unverified.
- OQ-31 Counsel review still required; findings are research, not legal advice.

## Darling-history minors

- OQ-32 Exact commit/date when the LKM was dropped as default (rationale
  known, timeline not).
- OQ-33 Whether GNUstep-based darling-appkit repos are active or dormant
  (READMEs absent/404); only Cocotron is wired as a submodule.

## Fidelity / determinism (docs 11, 14)

- OQ-34 Exact Apple-Silicon DTrace probe surface after `csrutil enable
  --without dtrace` (which `syscall:::return` probes still fault).
  Unblocks: Mac-side collector design (doc 14).
- OQ-35 Userspace-only Mach MIG semantic decoding: message-id to
  method-arg mapping without kernel hooks or private MIG tables.
  Unblocks: Mach logger at shim tier (doc 14).
- OQ-36 Single deterministic allocator pick: DieHard randomizes for
  safety; the harness needs a fixed-seed deterministic mode evaluated
  and chosen. Unblocks: bit-reproducible traces (doc 14).
- OQ-37 Virtual-clock + ARM time-counter implementation pick (rr's RCB
  counter is x86-only). Unblocks: determinism controls on ARM (doc 14).
- OQ-38 Record/replay capture must be macOS-native (dtrace/xctrace):
  rr is Linux-only, so rr covers the Linux replay side and the
  trace-portability design only. Unblocks: P4 replay scope (doc 14).
- OQ-39 Syscall-level differ vs libSystem-boundary fidelity:
  libSystem-to-kernel is private/no-ABI-guarantee, so raw trace
  equality cannot gate releases; gate rule needs sign-off (doc 11).
- OQ-40 `DYLD_SHARED_CACHE_DIR` determinism override vs
  PrebuiltLoaderSet never-with-overrides / invalid-after-reboot
  policy: harness must pick one cache mode per run (doc 12).
- OQ-41 DTrace collector needs recovery-mode csrutil exemption:
  incompatible with GHA-hosted runners, unconfirmed on Tart/EC2 VMs
  (docs 14, 15).
- OQ-42 Per-release Apple libm ulp tables + Accelerate per-CPU
  dispatch map (only divergence existence verified). Unblocks: T1
  enforcement (doc 11).
- OQ-43 Apple ICU fork patch list vs upstream (only fork existence +
  version tags verified). Unblocks: T0 ICU pinning (doc 11).
- OQ-44 CFAbsoluteTime epoch/calendars + NSLocale calendar list from a
  fetchable primary (developer docs JS-blocked).
- OQ-45 SBPL profile-language grammar + App Sandbox entitlement
  catalog from a fetchable primary (JS-blocked); no sandbox profile
  corpus or XNU sandbox suite cited. Unblocks: T2 sandbox matrix +
  G3 XPC/sandbox conformance.
- OQ-46 Raw syscall error convention (carry flag) + Mach trap numbers
  from a fetchable primary (only BSD syscalls.master verified).
- OQ-47 pboard/pasteboard-server wire protocol + promised-file staging
  semantics (only client API behavior verified).
- OQ-48 PF_SYSTEM kernel-event protocol family details beyond
  `AF_SYSTEM=32`.
- OQ-49 Lamport/causal merge of unfinished/resumed pairs unvalidated
  for Mach IPC causality. Unblocks: multi-threaded Mach gating.
- OQ-50 Literal default cache-dir constant
  `MACOSX_MRM_DYLD_SHARED_CACHE_DIR` value (expected
  `/System/Library/dyld`); macro site not found in dyld repo.
- OQ-51 Foundation/AppKit internal bring-up (`NSApplicationMain`
  sequence): closed source, current dev pages JS shells — specify
  from behavior + golden traces.
- OQ-52 `_libxpc_initializer` internals (bootstrap port lookup order):
  libxpc source unpublished; bring-up at API level only.
- OQ-53 libsystem_malloc `__malloc_init` internals: call-site order
  verified, allocator/zone source not inspected.

## GUI / media / perf gates (docs 11, 13, 16)

- OQ-54 No NSEvent-timestamp / runloop-ordering / vsync timing
  fidelity tier; reg-suit covers pixels + scripted events only.
- OQ-55 GPU golden images need a vendor-tolerance metric
  (SSIM/perceptual) + allowlist; raw per-pixel gating contradicts
  allowed hw-dispatch divergence.
- OQ-56 Audio: no latency/jitter/buffer-size/sample-clock tier and no
  differential audio method (only headless CoreAudio render named).
- OQ-57 APFS insensitive/normalization test corpus missing; L0
  overlayfs is case-sensitive with no insensitive-lookup emulation
  plan.
- OQ-58 Perf gates (3% no-regression, 10% cross-arch slip) vs ~8.94%
  TSO variance + Accelerate per-CPU / GCD auto-split nondeterminism:
  needs pinned oracle config + variance budget before binding.
- OQ-59 No pinned oracle hardware; no same-hw vs normalized cross-hw
  parity definition; hyperfine is CLI-only.
- OQ-60 Page-size (16K/4K) missing from the M6/G6 translator checklist
  (endian+32/64 only).

## Oracle cost / licensing (doc 15)

- OQ-61 New Mac mini MSRP unverified from primary (apple.com JS;
  secondaries: M4 $599 discontinued, M6 from $899).
- OQ-62 Used Intel Mac pricing/availability: no primary inspected.
- OQ-63 Orka Desktop free-tier current status: secondary only.
- OQ-64 Scaleway Apple Silicon hourly rates: secondary only.
- OQ-65 EC2 24h-minimum idle billing vs GHA per-minute unmodeled.
- OQ-66 Tart OCI-registry macOS image redistribution vs SLA 2-VM
  limit unexamined; golden-trace check-in redistribution risk
  unexamined.

## Test-infra specifics (docs 13, 16)

- OQ-67 Wine Conformance-Tests wiki body JS-gated (findings rest on
  test sources + dashboard); Writing_Conformance_Tests 403 via bot
  gate.
- OQ-68 No ready-made MIG `.defs` grammar fuzzer harness;
  MACHFUZZER paper is design only, harness must be authored (XL).
- OQ-69 Apple GUI snapshot/XCTest infra specifics not inspected;
  reg-suit is a generic substitute.
- OQ-70 docs.darlinghq.org loader-internals page body unfetched
  (timeout); covered via README + dyld4.md + Mach-O topics.
- OQ-71 AppDB Gold/Silver/Bronze/Garbage exact definitions
  uninspected (homepage shows Platinum only).
- OQ-72 Apple Code Signing Guide archive URL 404; covered via
  codesign(1) man page.
- OQ-73 Geekbench 6 benchmark-internals doc 404; release-benchmark
  gate grounded on LLVM test-suite instead.

## Harness agents / virtualization (docs 17–22)

- OQ-74 ARM64 KVM nested-virtualization exact requirements
  (FEAT_NV2/Armv8.4, kernel flags) not verified against primary
  kernel docs. Unblocks: nested-KVM planning (currently: never plan it).
- OQ-75 VirtualBox ARM host/guest parity and Guest Additions ARM
  status only in secondary sources, not the virtualbox.org manual.
- OQ-76 Tart headless Linux-desktop screenshot/video capture
  specifics not found in Tart docs inspected.
- OQ-77 Apple Virtualization.Framework nested-virt availability not
  verifiable (developer page JS-gated).
- OQ-78 No single primary-source schema for YAML/TOML test defs;
  the doc 17 schema is harness-authored.
- OQ-79 Normalization rule set is harness-authored; no primary
  source standardizes cross-OS result normalization.
- OQ-80 Current Apple AXUIElement API reference body not readable
  (developer.apple.com requires JavaScript; only title shell
  verified).
- OQ-81 Dogtail current docs not inspected
  (dogtail.readthedocs.io 404; gitlab README raw path 404).
- OQ-82 pyatspi2 README not inspected (guessed raw path 404).
  Medium-tier Linux tree assertions rest on uninspected APIs.
- OQ-83 No primary source gives exact numeric cross-platform
  GPU/font thresholds; Playwright mandates per-platform baselines
  instead. Unblocks: numeric pixel gates (currently: same-platform
  baselines only).
- OQ-84 Current MacStadium list prices beyond Feb-2026 verified
  figures (vendor moved to contact-sales).
- OQ-85 Exact AWS on-demand $/hr for M4/M4 Max hosts (inspected
  comparison figures used; AWS page confirms the model, not rates).
- OQ-86 Checks API endpoint details not directly fetched
  (referenced from the inspected status-checks doc).
- OQ-87 Hosted-runner screencapture/TCC Screen-Recording behavior
  not directly verified on GHA macOS images.
- OQ-88 Tart guest command-exec mechanism (VZ vs SSH-only) not
  verified in Tart docs; qemu-ga inside Tart VMs is unconfirmed.
- OQ-89 ScreenCaptureKit SCStream API surface still JS-gated;
  SMAppService registration uninspected.
- OQ-90 Long-term dashboard store unchosen (nightly metrics sink).
- OQ-91 QEMU savevm/HMP vs libvirt snapshot-path performance and
  qcow2 internal-snapshot locking under parallel jobs unmeasured.
- OQ-92 Agent pkg notarization/stapling install path on fresh
  minis not inspected.
- OQ-93 DTrace csrutil exemption confirm on Tart/EC2 VMs still
  untested.
- OQ-94 Mach IPC shim logger msg-id decode still sourceless.
- OQ-95 Artifact upload transport has no Apple primary source;
  generic CI store over HTTPS assumed.
- OQ-96 No canonical primary source for per-milestone corpus
  growth policy; doc 20 growth tiers are a proposal.
- OQ-97 No inspected primary source for a ready-made macOS GUI
  smoke-app corpus; Darling only confirms GUI-in-progress status.

(Conformance-corpus full evidence was recovered during synthesis —
no separate gap retained for it.)

## Dependency / supply-chain gaps (docs 23–28)

(System-deps-FFI and vendoring-supply-chain part-files were recovered
during synthesis — no separate gaps retained for those two.)

- OQ-98 Raw io_uring kernel/liburing handroll steps not fetched from
  kernel primary docs; covered via the mio reactor abstraction only.
- OQ-99 Apple ColorSync private-CMM numeric parity deltas vs LittleCMS2
  not inspected (no Apple doc fetched).
- OQ-100 Codec patent-licensing exposure for H.264/AAC distribution not
  verified against licensor terms.
- OQ-101 ICU4C vs ICU4X size/perf tradeoff for the compat layer not
  inspected.
- OQ-102 Rust Design Patterns newtype/wrapper page not located (two URL
  shapes 404; book restructured).
- OQ-103 No primary-source Rust essay on minimal-dependency maintenance
  (e.g. dtolnay-style) found via search; guidance synthesized from the
  Cargo Book and API guidelines instead.
- OQ-104 No Mac-behavior oracle corpus inspected; differential-testing
  guidance extrapolated from the musl-glibc diff log and Redox suites.
- OQ-105 MSRV/edition values for goblin/object/tokio/mio/clap/serde/cxx/
  bindgen/ash/wgpu/objc2 not fetched from manifests. Conflicts with the
  pinned toolchain + direct-minimal-versions floor CI.
- OQ-106 Build-dep + host-dep full tree (`cargo tree -e all` incl.
  build/proc-macro) not measured for any verdict; node counts are
  normal-edge only.
- OQ-107 Ubuntu SRU/ABI policy + Launchpad PPA no-network text still
  unfetched (both prior timeouts).
- OQ-108 FFmpeg/BoringSSL license compatibility with the deny allow-list
  + debian/copyright burden not verified.
- OQ-109 Handrolled-parser security maintenance cost (fuzz corpus, CVE
  response) vs adopted-crate RustSec coverage not quantified.
- OQ-110 tracing/tracing-subscriber tree cost unmeasured; the log-vs-
  tracing reconciliation rule has no leaf-cost comparison.
- OQ-111 wgpu default-features total node count not measured; object 0.40
  vs measured 0.36 tree delta unverified.
- OQ-112 Incremental cost of clap unicode/wrap_help and bindgen without
  prettyplease not measured; cxx C++ compile-time/build cost not
  quantified (nodes only).
- OQ-113 yeslogic-fontconfig-sys docs page 404 (wrapper page used
  instead); distro patches vs docs.rs upstream never diffed; Mesa
  EGL/libEGL + libpipewire-0.3 noble pages unfetched.
- OQ-114 Per-lib static-link feasibility inferred from package dep lists,
  not from a link trial.
- OQ-115 cargo vet / cargo crev audit-trail tooling not inspected; policy
  covers audit+deny only.
- OQ-116 Vendor-tarball-in-`.dsc` vs Debian no-convenience-copies rule:
  direct contradiction; PPA acceptance unresolved.
- OQ-117 TLS/SecureTransport fidelity surface missing from the doc 11
  F-table: define tier (T0 wire bytes? T1?) + comparator + cert-
  validation parity before P3.2 gates on it.
- OQ-118 Which mechanism provides F7 case-insensitive lookup semantics on
  ext4 (FUSE overlay, ciopfs-style layer, in-shim translation)?
  Needed before the P2.3 fs-fidelity suites bind.
- OQ-119 xctrace `.trace` files evaporate with the CI runner (only the
  path is recorded). Decide the export/summary format for `.trace`
  goldens before P1 differential work depends on them.
