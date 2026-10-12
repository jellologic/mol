# 09 — Roadmap and Milestones

Phasing: **CLI first, then GUI**, on **Ubuntu Desktop for ARM64 + AMD64**.
Each milestone lists exit criteria and the compat tier it unlocks.

Compat tiers (Darling-anchored):
Tier 1 CLI tools first (e.g. jq, curl, ffmpeg macOS binaries);
Tier 2 headless dev-toolchain binaries (e.g. python3, git, node);
Tier 3 simple GUI apps last (e.g. Calculator-class, TextEdit-class, one OSS
Cocoa app). Sources:
[darling](https://github.com/darlinghq/darling).

## M0 — Skeleton + legal + CI (weeks 1–4)

- Crate skeleton (doc 07), `xtask` codegen, relibc-style `check.sh`,
  C-ABI conformance harness, golden-guest corpus seed.
- Resolve license posture (GPL-3.0 vs permissive, doc 08) and stub-gen legality.
- cargo-deb + PPA pipeline (source .dsc, vendored crates) on both arches.
- Exit: `mol-loader --help` + empty-prefix boot on AMD64 and ARM64 Ubuntu Desktop.

## M1 — Same-ISA CLI core (matrix #2, then #1)

- Mach-O/fat parse, segment map + slide, dyld bind/chained-fixups engine,
  BSD syscall table, minimal mol-server (process/thread mgmt, bootstrap
  registry), launchd-as-init in overlayfs prefix.
- ObjC messenger + category/`+load` order; Swift runtime dylibs.
- Exit (Tier 1, AMD64 first): macOS `jq`, `curl`, `ffmpeg` CLI binaries run;
  then same on ARM64 (matrix #1).

## M2 — IPC depth + headless toolchain (Tier 2)

- Full Mach IPC (rights, port sets, OOL), MIG task/thread/exception ports,
  VM pager/COW nuance, XPC + launchd socket activation, notifyd, libdispatch
  Linux port, CFRunLoop on epoll.
- IOKit stubs on demand for toolchain needs.
- Exit (Tier 2, both arches): macOS `python3`, `git`, `node` run headless;
  Homebrew formulae build hello-world-class packages.

## M3 — Text/media foundations (GUI prerequisites)

- CoreText over HarfBuzz/FreeType/Fontconfig; CoreGraphics-2D over
  Cairo/Skia; PipeWire audio; GStreamer/FFmpeg AVFoundation core;
  WebKitGTK embed; Wayland xdg-shell + decoration + fractional-scale +
  text-input protocols.
- Exit: headless render tests (PDF→PNG, text layout, audio sine, video
  transcode) pass on both arches.

## M4 — GUI shell (Tier 3, first pixels)

- CoreAnimation layer tree → Vulkan scene graph / Wayland subsurfaces;
  AppKit event loop + responder chain + minimal widget set.
- Exit (Tier 3, both arches): Calculator-class and TextEdit-class apps
  usable; one OSS Cocoa app runs. Note Darling's 10-year hello-world-only
  precedent — scope ruthlessly (doc 02).

## M5 — Metal + MSL over Vulkan

- Metal API re-implementation over `ash` core; new MSL→SPIR-V frontend;
  family feature-parity matrix as the tracking artifact (doc 10).
- Exit: triangle + compute samples; one simple Metal game's menu renders.
- UIKit/Catalyst explicitly after M5.

## M6 — Cross-arch JIT (matrix #4, then #3)

- arm64-macOS on AMD64 via QEMU-TCG-class translator + mol loader.
- x86_64-macOS on ARM64 via IR JIT/dynarec + TSO emulation last.
- Exit: Tier 1 binaries run under translation; perf budget published.

## Milestone → tier summary

| Milestone | Arches | Tier unlocked |
|-----------|--------|---------------|
| M0 skeleton | ARM64 + AMD64 | — |
| M1 CLI core | AMD64, then ARM64 | Tier 1 |
| M2 IPC + toolchain | both | Tier 2 |
| M3 text/media | both | (prereq) |
| M4 GUI shell | both | Tier 3 |
| M5 Metal/MSL | both | Tier 3+ |
| M6 cross-arch | translated pairs | Tier 1 (translated) |
