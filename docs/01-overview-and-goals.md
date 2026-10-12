# 01 — Overview and Goals

## Goal

Run unmodified macOS binaries on Ubuntu Desktop Linux (ARM64 and AMD64)
without Apple hardware, emulation of the whole OS, or a kernel module.

## Non-goals (v1)

- Running Apple-signed system apps that require Secure Enclave, SEP-backed
  keychain, FairPlay DRM, or notarization/Gatekeeper enforcement.
- Kernel extensions, VM/hypervisor guests, AVX-512 guests under Rosetta-class
  translation.
- Bit-perfect APFS semantics on day one (see doc 10).

## Architecture at a glance

```text
macOS binary (Mach-O, fat/universal)
        │  parse + map segments + slide
        ▼
   mol-loader (mldr equivalent)
        │  hand off to dyld (bind, rebase, chained fixups, init order)
        ▼
   guest process ──BSD syscalls──► Linux syscalls (renumber + shim)
        │  Mach traps / MIG RPC
        ▼  Unix datagram socket, one per thread, SCM_RIGHTS
   mol-server (darlingserver equivalent: ports, tasks, VM, bootstrap)
        │  frameworks (Foundation/AppKit/Core* reimpl or Apple OSS forks)
        ▼
   host Linux: Vulkan, PipeWire, Wayland, GStreamer/FFmpeg, WebKitGTK
```

Sources: Darling loader model
([loader.md](https://github.com/darlinghq/darling-docs/blob/master/src/internals/basics/loader.md)),
syscall emulation
([doc](https://github.com/darlinghq/darling-docs/blob/master/src/internals/basics/system-call-emulation.md)),
darlingserver
([README](https://github.com/darlinghq/darling-docs/blob/master/src/internals/darlingserver/README.md),
[RPC](https://github.com/darlinghq/darling-docs/blob/master/src/internals/darlingserver/rpc.md)),
prefixes
([doc](https://github.com/darlinghq/darling-docs/blob/master/src/internals/basics/containerization.md)).

## Component effort map

| Component | Tier | Notes |
|-----------|------|-------|
| Mach-O loader + dyld bind engine | M–L | Reuse Apple OSS dyld sources; see doc 03 |
| BSD syscall translation | M | Table + shims for Darwin-only calls |
| Mach IPC + MIG + exceptions | XL | Largest kernel-side item; see doc 04 |
| launchd + XPC + notifyd | L | AF_UNIX daemons; see doc 04 |
| ObjC/Swift runtimes | L | Apple OSS objc4 + Swift dylibs; see doc 03 |
| Foundation/CoreFoundation | L–XL | Forks + shims; see doc 05 |
| AppKit/UIKit GUI | XL | Dominates schedule; see doc 05 |
| Metal + MSL over Vulkan | XL | New MSL→SPIR-V frontend; see doc 05 |
| Media (audio/video/web) | S–L | Delegate to PipeWire/GStreamer/WebKitGTK |
| Cross-arch JIT (x86-on-ARM etc.) | XL | Phase-gated; see doc 06 |

## Rust-versus-C guidance (global)

- Default to Rust for all new code: loader, server, daemons, framework shims.
- Keep C only at: dyld entry/TLS asm, commpage setup, signal/exception
  trampolines, and thin FFI shims over reused Apple C/C++/ObjC sources.
- Confine `unsafe` to `-sys`/bridge crates; guest-mapped code paths stay
  synchronous (no async across the Mach-O boundary). Details in doc 07.
