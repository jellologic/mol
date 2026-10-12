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
