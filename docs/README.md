# mol — macOS Compatibility Layer on Linux (Design Docs)

Research synthesis for building a Darling-style macOS loader + syscall/IPC server
in Rust, targeting Ubuntu Desktop (ARM64 + AMD64), CLI first, GUI later.

## Index

- [01 — Overview and goals](01-overview-and-goals.md)
- [02 — Prior art](02-prior-art.md)
- [03 — Binary loader and runtimes](03-binary-loader-and-runtimes.md)
- [04 — Kernel ABI and IPC](04-kernel-abi-and-ipc.md)
- [05 — Frameworks, GUI, media](05-frameworks-gui-media.md)
- [06 — Cross-arch strategy](06-cross-arch-strategy.md)
- [07 — Rust architecture](07-rust-architecture.md)
- [08 — Legal and distribution](08-legal-and-distribution.md)
- [09 — Roadmap and milestones](09-roadmap-milestones.md)
- [10 — Open questions](10-open-questions.md)
- [11 — Fidelity contract](11-fidelity-contract.md)
- [12 — Foundation build order](12-foundation-build-order.md)
- [13 — Validation and testing](13-validation-and-testing.md)
- [14 — Differential harness](14-differential-harness.md)
- [15 — Oracle strategy](15-oracle-strategy.md)
- [16 — Milestone gates](16-milestone-gates.md)
- [17 — Test harness overview](17-test-harness-overview.md)
- [18 — macOS test agent](18-macos-test-agent.md)
- [19 — Ubuntu VM test agent](19-ubuntu-vm-test-agent.md)
- [20 — Harness orchestration](20-harness-orchestration.md)
- [21 — GUI validation](21-gui-validation.md)
- [22 — Harness CI integration](22-harness-ci-integration.md)
- [23 — Dependency philosophy](23-dependency-philosophy.md)
- [24 — Rust crate audit](24-rust-crate-audit.md)
- [25 — Handroll playbook](25-handroll-playbook.md)
- [26 — Never handroll](26-never-handroll.md)
- [27 — System deps and FFI](27-system-deps-and-ffi.md)
- [28 — Vendoring and supply chain](28-vendoring-and-supply-chain.md)
- [29 — Implementation plan](29-implementation-plan.md)
- [30 — Phase P0: foundation](30-phase-P0-foundation.md)
- [31 — Phase P1: CLI core](31-phase-P1-cli-core.md)
- [32 — Phase P2: IPC + toolchain](32-phase-P2-ipc-toolchain.md)
- [33 — Phase P3: text + media](33-phase-P3-text-media.md)
- [34 — Phase P4: GUI shell](34-phase-P4-gui-shell.md)
- [35 — Phase P5: Metal](35-phase-P5-metal.md)
- [36 — Phase P6: cross-arch](36-phase-P6-cross-arch.md)
- [37 — Phase PG: hardening](37-phase-PG-hardening.md)
- [38 — Sub-phase P0.1: legal + policy](38-subphase-P0.1-legal-policy.md)
- [39 — Sub-phase P0.2: skeleton + CI](39-subphase-P0.2-skeleton-ci.md)
- [40 — Sub-phase P0.3: prefix + packaging](40-subphase-P0.3-prefix-packaging.md)
- [41 — Sub-phase P0.4: harness + traces](41-subphase-P0.4-harness-traces.md)
- [42 — Sub-phase P1.1: loader + dyld](42-subphase-P1.1-loader-dyld.md)
- [43 — Sub-phase P1.2: libSystem + syscalls](43-subphase-P1.2-libsystem-syscalls.md)
- [44 — Sub-phase P1.3: ObjC + Swift](44-subphase-P1.3-objc-swift.md)
- [45 — Sub-phase P1.4: server + Tier-1 exit](45-subphase-P1.4-server-tier1-exit.md)
- [46 — Sub-phase P2.1: Mach IPC + VM](46-subphase-P2.1-mach-ipc-vm.md)
- [47 — Sub-phase P2.2: XPC + launchd + GCD](47-subphase-P2.2-xpc-launchd-gcd.md)
- [48 — Sub-phase P2.3: Foundation + toolchain exit](48-subphase-P2.3-foundation-toolchain-exit.md)
- [49 — Sub-phase P3.1: text + 2D](49-subphase-P3.1-text-2d.md)
- [50 — Sub-phase P3.2: audio + video + web](50-subphase-P3.2-audio-video-web.md)
- [51 — Sub-phase P3.3: Wayland + tolerance + fuzzer](51-subphase-P3.3-wayland-tolerance-fuzzer.md)
- [52 — Sub-phase P3.4: headless exit](52-subphase-P3.4-headless-exit.md)
- [53 — Sub-phase P4.1: compositing + event loop](53-subphase-P4.1-compositing-event-loop.md)
- [54 — Sub-phase P4.2: widgets + timing](54-subphase-P4.2-widgets-timing.md)
- [55 — Sub-phase P4.3: Tier-3 exit](55-subphase-P4.3-tier3-exit.md)
- [56 — Sub-phase P5.1: Metal API + matrix](56-subphase-P5.1-metal-api-matrix.md)
- [57 — Sub-phase P5.2: MSL frontend](57-subphase-P5.2-msl-frontend.md)
- [58 — Sub-phase P5.3: game menu exit](58-subphase-P5.3-game-menu-exit.md)
- [59 — Sub-phase P6.1: matrix #4](59-subphase-P6.1-matrix4.md)
- [60 — Sub-phase P6.2: matrix #3 + TSO](60-subphase-P6.2-matrix3-tso.md)
- [61 — Sub-phase P6.3: ratios exit](61-subphase-P6.3-ratios-exit.md)
- [62 — Sub-phase PG.1: hygiene + sanitizers + fuzz](62-subphase-PG.1-hygiene-sanitizers-fuzz.md)
- [63 — Sub-phase PG.2: benchmarks + release](63-subphase-PG.2-benchmarks-release.md)
- [64 — Tracker tooling](64-tracker-tooling.md)

## Effort-tier legend (used in every doc)

| Tier | Meaning |
|------|---------|
| S | Dependency / embed an existing crate or library |
| M | Shim over a Linux equivalent |
| L | Subsystem re-implementation |
| XL | Full framework re-implementation |

## One-paragraph architecture

`mol-loader` (Mach-O parse + map, in-tree) loads the guest binary and dyld;
a rewritten `libsystem_kernel` translates BSD syscalls to Linux and forwards
Mach traps over per-thread Unix sockets to `mol-server`, a userspace kernel
server (Darling's darlingserver model) that owns ports/rights, tasks/threads,
VM bookkeeping, and bootstrap/launchd registries; userland reuses Apple
open-source drops (Libc, dyld, objc4) plus reimplemented frameworks
(Foundation, AppKit, CoreFoundation, Metal-on-Vulkan); prefixes are
mount+PID-namespace containers with overlayfs and launchd-as-init.
