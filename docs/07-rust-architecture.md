# 07 — Rust Architecture

## Proposed crate layout

| Crate | Role | Key deps |
|-------|------|----------|
| mol-macho | Mach-O/fat parse | goblin (+ object for write paths only) |
| mol-loader | Segment map + slide | memmap2 |
| mol-dyld | Bind/opcode engine; C entry shim only | mol-macho |
| mol-syscall | nix-backed BSD table + Mach-trap stubs | nix |
| mol-mach | Ports/rights/port-sets, tokio reactor | tokio |
| mol-xpc | XPC dict/array wire over mol-mach | mol-mach, tokio |
| mol-libsystem | relibc-style libSystem reimplementation | nix, libc |
| mol-objc | objc2/block2/dispatch2 re-export + runtime shims | objc2, block2, dispatch2 |
| mol-metal-vk | Metal API over Vulkan core | ash (+ vulkano optional) |
| mol-launchd | plist spawn + bootstrap registry on tokio | tokio |
| mol-bridge | elf_calls-style host-wrap table + wrapgen equivalent | — |
| xtask | Codegen, fixtures | — |

## Dependency evidence

- `goblin`: zero-copy endian-aware 32/64-bit Mach-O parser, fuzzed 100M runs:
  [README](https://raw.githubusercontent.com/m4b/goblin/master/README.md).
- `object`: unified read API + Mach-O/ELF/PE write, but **trusted-inputs-only**,
  unfit for hostile binaries:
  [README](https://raw.githubusercontent.com/gimli-rs/object/master/README.md).
- `memmap2`: file-backed/executable/CoW maps:
  [README](https://raw.githubusercontent.com/RazrFalcon/memmap2-rs/master/README.md).
- `nix`: Rust-friendly \*nix bindings structured by C header:
  [docs](https://docs.rs/nix/latest/nix/).
- `tokio`: work-stealing scheduler + epoll/kqueue reactor:
  [README](https://raw.githubusercontent.com/tokio-rs/tokio/master/README.md).
- `objc2` merged repo: framework crates + `block2` (C blocks) + `dispatch2`
  (GCD, Linux targets):
  [README](https://raw.githubusercontent.com/madsmtm/objc2/main/README.md),
  [block2](https://docs.rs/block2/latest/block2/),
  [dispatch2](https://docs.rs/dispatch2/latest/dispatch2/).
- `ash`: lightweight unsafe Vulkan wrapper from vk.xml, Vulkan 1.1–1.4:
  [README](https://raw.githubusercontent.com/ash-rs/ash/master/README.md).
- `vulkano`: safe wrapper, prevents invalid API use, automatic GPU sync:
  [README](https://raw.githubusercontent.com/vulkano-rs/vulkano/master/README.md).
- `wgpu`: safe pure-Rust graphics over Vulkan/Metal/DX12/GL (WebGPU-based),
  alternative backend: [README](https://raw.githubusercontent.com/gfx-rs/wgpu/trunk/README.md).

## FFI strategy

- `cxx` bridge for C++ (Vulkan/MoltenVK-adjacent, host GL/VK), mix-and-match
  with `bindgen` for oddball signatures: [cxx](https://docs.rs/cxx/latest/cxx/).
- `bindgen` for C/ObjC headers incl. Objective-C chapter:
  [intro](https://rust-lang.github.io/rust-bindgen/introduction.html).
- `repr(C)` + `cbindgen` for exported libSystem ABI.
- `unsafe` confined to `-sys`/bridge crates.

## Runtime rules

- tokio multi-thread for servers/launchd; guest-mapped code stays **sync**
  (no async across the Mach-O boundary).
- Mach messages framed over tokio Unix sockets.
- dyld reimplementation precedent `dryad` (Rust dynamic linker) stalled on
  cargo/musl shared-object linking pain — keep dyld entry/TLS-asm/commpage
  as C shims: [dryad](https://raw.githubusercontent.com/m4b/dryad/master/README.md).

## Lessons from relibc / Redox / Weave / Darling bridge / Cuttlefish

- relibc: portable POSIX libc in Rust with per-platform backends, `ld_so`
  loader code, cbindgen headers, C test suite:
  [README](https://gitlab.redox-os.org/redox-os/relibc/-/raw/master/README.md).
- Redox: compat lives in userland libc with clean per-platform modules, not a
  POSIX kernel: [book](https://doc.redox-os.org/book/libraries-apis.html).
- Weave: Rust compat layer delegates to host subsystems (DXVK/VKD3D, xcb,
  PipeWire) + thin ntdll shim, ~5–10% of Wine volume:
  [README](https://raw.githubusercontent.com/The-Libre-Project/Weave/main/README.md).
- Darling ELF bridge: mldr passes `struct elf_calls` via applep; wrapgen +
  Mach-O symbol resolvers wrap host libs:
  [doc](https://raw.githubusercontent.com/darlinghq/darling-docs/master/src/internals/calling-host-system-apis.md).
- Cuttlefish: configurable virtual device with full framework fidelity for
  CTS/CI testing — model for the golden-guest corpus:
  [docs](https://source.android.com/docs/devices/cuttlefish).

## Packaging

- `cargo-deb`: builds .deb from Cargo.toml + `[package.metadata.deb]`, with
  systemd-unit support and auto deps:
  [README](https://raw.githubusercontent.com/kornelski/cargo-deb/master/README.md).
- Ship multi-binary debs (mol-loader, mol-server, mol-launchd + systemd
  units); PPA via source .dsc with vendored crates for offline Launchpad builds.

## Testing

- relibc-style `check.sh` (unit + arch matrix), C-ABI conformance tests,
  Weave-style `status.yaml` export registry, Cuttlefish-style golden guest
  corpus + CTS-like CI gate.

## Per-crate tiers

S: mol-macho, mol-loader (deps-led). M: mol-syscall, mol-xpc, mol-launchd,
mol-bridge. L: mol-dyld, mol-mach, mol-libsystem, mol-objc, mol-metal-vk core.
Metal API completeness and MSL frontend escalate to XL (doc 05).
