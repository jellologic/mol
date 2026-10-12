# 16 — Milestone Gates

Engineer checklists: CLI-first then GUI, same-ISA before cross-arch.
Gate numbering G1–G7 maps onto doc 09 milestones M1–M6 + hardening;
doc 09 stays the schedule, this doc is the exit-criteria detail.

Mapping: G1→M1(loader part), G2→M1(CLI part), G3→M2, G4→M3, G5→M4,
G6→M5+M6, G7 = release hardening (runs at every milestone boundary).

## G1 — Loader skeleton (Tier: L)

- [ ] Parse header + load commands of thin+fat Mach-O incl
  `LC_LOAD_DYLINKER`. (Mach-O = header + load commands + segment
  data: [Mach-O topics](https://developer.apple.com/library/archive/documentation/DeveloperTools/Conceptual/MachOTopics/0-Introduction/introduction.html))
- [ ] Map segments per VM layout, apply rebase/bind opcodes.
- [ ] Transfer to entry with the Darwin stack layout
  (argc/argv/envp/apple params); kernel pushes params then jumps to
  dyld entry, one Loader per Mach-O
  (PrebuiltLoader vs JustInTimeLoader):
  [dyld4.md](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/doc/dyld4.md).
- [ ] `loader hello-static` prints via direct syscalls.
- Suites: Mach-O corpus (thin, fat, PIE, dyld-cache-present),
  dyld_info/lipo cross-checks, FileCheck golden traces of load maps
  (FileCheck matches multiple patterns in order:
  [FileCheck](https://llvm.org/docs/CommandGuide/FileCheck.html)).
- DoD: zero-diff load maps vs `otool -l`/`dyld_info` on corpus.

## G2 — Same-ISA CLI core (Tier: M–L)

- [ ] libSystem shim passes syscall smoke
  (open/read/write/mmap/sigaction/posix_spawn).
- [ ] `shell echo`, cat, ls, `python3 -c`, /bin/sh scripts run.
  (Darling's hello gate is `darling shell echo Hello world`,
  DPREFIX chroot like WINEPREFIX:
  [Darling README](https://raw.githubusercontent.com/darlinghq/darling/master/README.md);
  Wine's smoke gate is `./wine notepad`:
  [Wine README](https://raw.githubusercontent.com/wine-mirror/wine/master/README.md).)
- [ ] DPREFIX-equivalent init + overlay isolation (doc 12, L0).
- Suites: per-module conformance tests (Wine `dlls/*/tests` pattern,
  e.g. [ntdll info.c](https://raw.githubusercontent.com/wine-mirror/wine/master/dlls/ntdll/tests/info.c)),
  LTP-analog Darwin syscall subset, 50-binary CLI corpus pass/fail board.
- DoD: Tier-1 candidates exit 0 with byte-identical stdout vs native macOS.

## G3 — IPC depth + headless toolchain (Tier: XL for Mach IPC, M–L rest)

- [ ] Mach IPC (ports/rights/messages) + GCD/launchd lifecycle
  (on-demand, restart-on-crash, kill-when-idle). (XPC is part of
  libSystem; services stateless, each sandboxed; ObjC via
  NSXPCConnection:
  [XPC services](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html).)
- [ ] XPC request/reply incl crash-mid-reply invalid-connection semantics.
- [ ] Headless toolchain: clang/cc1 + ld + `codesign -s -` +
  installer/hdiutil mount .pkg/.dmg (Xcode toolchain payload installs;
  IDE need not run). (`codesign -v` verifies, default
  `--all-architectures` on fat, `-a` selects one arch:
  [codesign.1](https://www.manpagez.com/man/1/codesign/).)
- Suites: XPC conformance (privilege-separation/sandbox attrs
  asserted), launchd lifecycle fault-injection, toolchain build of
  hello + static lib + `codesign -v` verify.
- DoD: toolchain-produced binaries re-enter G1/G2 gates clean.

## G4 — Text + media foundations (Tier: L)

- [ ] CoreText shaping/render of ASCII+CJK+emoji, RTL/LTR matches
  reference bitmaps within tolerance.
- [ ] CoreGraphics paths/gradients/images/compositing golden-image
  diffs pass. (Cocoa drawing stack gates: contexts, coords/transforms,
  color/transparency, paths, images, text, Quartz/OpenGL/QuickTime:
  [drawing guide](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaDrawingGuide/Introduction/Introduction.html).)
- [ ] CoreAudio headless render + ImageIO decode png/jpeg/tiff.
- Suites: golden-image corpus with per-pixel-diff threshold +
  FileCheck stderr traces.
- DoD: no golden-image diff beyond threshold on corpus; text suite
  green on same-ISA.
- Open: GPU images need a vendor-tolerance metric (SSIM/perceptual)
  with an allowlist — raw per-pixel gating contradicts the allowed
  hw-dispatch divergence (doc 11); audio has no latency tier yet.

## G5 — GUI bring-up (Tier: XL)

- [ ] Event loop + window + view hierarchy + first-responder chain.
- [ ] Menu bar, alerts, file-open panel, drag-drop paths.
- [ ] Cocoa Drawing chapters smoke (contexts, transforms, color,
  paths, images, text).
- Suites: scripted GUI driver (launch, screenshot, synthetic events)
  + Metal/Vulkan backend triangles/compute golden frames. Example
  apps: TextEdit-clone, Calculator, image viewer, Notes. (Darling
  runs console apps — mc, Unarchiver, Python — with basic Cocoa GUI:
  [Darling](https://en.wikipedia.org/wiki/Darling_(software)).)
- DoD: example apps launch unaided, survive the scripted session,
  exit clean.
- Open: no NSEvent-timestamp / runloop-ordering / vsync timing tier.

## G6 — Cross-arch (Tier: XL)

- [ ] check-tcg-analog: same conformance corpus built+run under
  user-mode emulation for every enabled guest arch. (`make check-tcg`
  builds+runs tests under QEMU per guest arch:
  [QEMU testing](https://wiki.qemu.org/Testing).)
- [ ] Syscall translator covers endian + 32/64-bit size adjust + ioctl
  adjust. (QEMU user-mode: generic translator, signal
  synthesis/redirect, clone threading; self-emulation is the power
  test: [user mode](https://www.qemu.org/docs/master/user/main.html).)
- [ ] Rosetta-parity report: translated binary within perf budget of
  native, correctness equal. (Rosetta translates in background; from
  macOS 28 only for older unmaintained games; Universal preferred:
  [support](https://support.apple.com/en-us/102527).)
- Suites: per-arch LTP-style syscall runs in chroots (LTP validates
  QEMU linux-user per arch via debootstrap chroot:
  [LTP](https://wiki.qemu.org/Testing/LTP)), self-hosting test,
  Universal/fat per-arch `codesign -v --all-architectures`.
- Functional tests as Python unittest+TAP (`QemuSystemTest`/
  `QemuUserTest` via `make check-functional[-target]`):
  [functional](https://www.qemu.org/docs/master/devel/testing/functional.html).
- DoD: per-arch dashboard green; no arch-gated instruction/config escapes.
- Open: page-size (16K/4K) missing from the translator checklist
  (endian+32/64 only) — add per doc 15 fidelity facts.

## G7 — Hardening, every release (Tier: M)

- [ ] `codesign -v` clean on all shipped binaries incl fat;
  entitlements/sandbox profiles reviewed.
- [ ] Valgrind leak-clean + UBSan/ASan clean on G2+G5 suites.
  (`make check` = check-qapi-schema + check-unit + check-qtest +
  check-decodetree + check-block; dynamic gates via Valgrind + UBSan:
  [QEMU testing](https://wiki.qemu.org/Testing).)
- [ ] Fuzz crashers at zero for loader/IPC/image parsers over budget.
- Suites: `make check` analog (unit+qtest+block equivalents),
  sanitizer CI leg, device-crash-test-analog matrix.
- DoD: release ships with SBOM + signed artifacts + security log review.

## Regression policy (no-go on golden-trace diffs)

Every gate owns golden traces (load maps, syscall traces, XPC
transcripts, pixels, frames) verified FileCheck-style in order.
Per-build dashboard (Wine pattern: 887 unit tests per build across
Win8..Win11 + Linux with error/todo counts:
[test.winehq.org](https://test.winehq.org/data/),
[AppDB 30,135 apps](https://appdb.winehq.org/)) must show zero new
failures vs baseline; any golden diff blocks merge until the trace is
re-baselined with written justification + reviewer sign-off.

## Compat-tier promotion (AppDB-Platinum-modeled)

Platinum = installs and runs flawlessly on out-of-the-box Wine:
[AppDB](https://appdb.winehq.org/).

- Tier 1 CLI tools (mc, unarchiver CLI, python3, git, ffmpeg-cli):
  install+run flawlessly out-of-box, byte-identical CLI output,
  3 consecutive green dashboards.
- Tier 2 headless toolchain (clang builds of cmake/ninja/make
  projects, pkgutil/installer flows, codesign flows): self-host +
  corpus builds green + signed output verifies.
- Tier 3 GUI apps (TextEdit-clone, Calculator, image viewer, then
  VLC/Blender-class): scripted GUI session + golden frames pass, no
  manual workaround beyond documented setup.
- Demotion on any release with a new golden diff or crash.
- Open: AppDB Gold/Silver/Bronze/Garbage exact definitions
  uninspected (homepage shows Platinum only).

## Release-gating benchmarks

LLVM test-suite ships reference outputs for correctness plus
runtime/compile-time/size metrics
(SingleSource/MultiSource/MicroBenchmarks/External-SPEC):
[guide](https://llvm.org/docs/TestSuiteGuide.html).

- [ ] SingleSource+MultiSource correctness 100% on Tier-1/2 paths,
  reference-output hashes match.
- [ ] MicroBenchmarks trend: no >3% regression vs last release on
  loader-startup, IPC round-trip, text-shape, blit.
- [ ] App-level: Tier-1 corpus wall-time, Tier-3 scripted-session
  fps/frame-time p95.
- [ ] Cross-arch: translated-vs-native ratio reported per benchmark;
  release notes flag any >10% slip.
- All numbers from a `make check-functional`-style harness with
  cached assets and TAP logs.
- Open tension: 3%/10% gates vs ~8.94% TSO variance + Accelerate
  per-CPU / GCD auto-split nondeterminism — needs a pinned oracle
  config + variance budget before these gates bind (doc 10, doc 15).
