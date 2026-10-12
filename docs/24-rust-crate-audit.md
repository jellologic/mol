# 24 — Rust Crate Audit

Per-crate adopt/handroll/replace verdicts with measured tree costs.
Method: node counts are unique normal-edge `cargo tree -p <crate>` nodes,
measured Oct 2026 on macOS/arm64 with latest compatible versions
(goblin 0.10.7, object 0.36.7, tokio 1.53.2, clap 4.6.7, wgpu 24.0.5,
objc2 0.6.5, cxx 1.0.205, bindgen 0.71.1, ash 0.38, nix 0.29,
memmap2 0.9.11, serde 1.0.229, log 0.4.34, anyhow 1.0.104).
Feature/no_std claims read from the linked manifests.

Global discipline: **`default-features = false` everywhere + explicit
features.** Biggest risks are the syn tax from any derive
(serde/clap/scroll/cxx), object default compression (flate2+ruzstd),
tokio full, bindgen libclang, and the wgpu naga floor.

## Verdict table

| Crate | Verdict | Tier | Minimal config |
|-------|---------|------|----------------|
| goblin XOR object | adopt-minimal, exactly one | S | goblin core-only, or object `read+macho+std` |
| memmap2 | adopt as-is | S | as-is (2 nodes) |
| nix | adopt-minimal | S | only needed module feats |
| tokio | replace-with-smaller | M | mio or std threads; else `rt+sync` only |
| serde | adopt-minimal | S | `default-features = false` (+alloc/derive as needed) |
| clap | replace-with-smaller | S | pico-args; else builder-only, no derive |
| log | adopt as-is | S | facade in all libs |
| anyhow | adopt as-is (bins only) | S | never in library API (erased errors) |
| cxx | handroll extern-C by default | M | adopt only for true C++ APIs |
| bindgen | build-dep only | M | pre-generate + check in; prefer hand/`objc2` crates |
| ash | adopt-minimal | S | no-default + `loaded` or `linked` |
| wgpu | replace-with-smaller | L | objc2-metal on macOS; else no-default `metal,wgsl` |
| objc2 | adopt-minimal | S | defaults fine |

## Detail

- **goblin:** default 11 nodes incl. `scroll_derive`→syn proc-macro chain;
  core-only elf64/no-std drops to 3 (self+plain+scroll). Tiers
  `std=[alloc,scroll/std]`, `alloc=[scroll/derive,log]`; per-format feats
  elf32/64 mach32/64 pe32/64 te archive. scroll derive is optional
  (`dep:scroll_derive`); avoiding the goblin alloc tier avoids syn entirely
  ([goblin](https://raw.githubusercontent.com/m4b/goblin/master/Cargo.toml),
  [scroll](https://raw.githubusercontent.com/m4b/scroll/master/Cargo.toml)).
- **object:** default 11 nodes via compression→flate2+ruzstd; minimal
  `read+macho+std` = 2 nodes (self+memchr). Granular
  read_core/write_core/std/compression/names/wasm; default
  `[read,compression]`; read_core is no_std-capable. Adopt-minimal exactly
  one of goblin/object, never both; object minimal tree is smaller for
  Mach-O-read-only
  ([manifest](https://raw.githubusercontent.com/gimli-rs/object/master/Cargo.toml)).
  Note doc 10 OQ-6: object is trusted-inputs-only.
- **memmap2:** 2 nodes (self+libc on unix); no `[features]` table,
  stable_deref optional; std-only. Adopt as-is; handroll libc mmap only
  for trivial single-map use
  ([manifest](https://raw.githubusercontent.com/RazrFalcon/memmap2-rs/master/Cargo.toml)).
- **nix:** `default = []` already minimal; base 4 nodes (libc+bitflags+
  cfg-if) + opt per-module feats (fs mman socket…); std-only.
  Adopt-minimal with only needed module feats; direct libc if 1–2 calls
  ([manifest](https://raw.githubusercontent.com/nix-rust/nix/master/Cargo.toml)).
- **tokio:** full = 21 nodes (mio/socket2/parking_lot/bytes/
  signal-hook-registry…); minimal `rt+sync` = 2 nodes
  (self+pin-project-lite); `default = []`. Std-only, no no_std tier
  ([manifest](https://raw.githubusercontent.com/tokio-rs/tokio/master/tokio/Cargo.toml)).
  Replace with mio (unix deps are libc + optional log only; granular
  os-poll/os-ext/net feats,
  [manifest](https://raw.githubusercontent.com/tokio-rs/mio/master/Cargo.toml))
  or std threads. Rule: `mio` Poll for the Mach reactor; tokio `rt+sync`
  only where spawned tasks genuinely help; never full.
- **serde:** no-derive = 2 nodes (serde_core); derive adds syn/quote chain
  = 9 nodes; tiers default std, plus alloc, core-only. serde_derive pulls
  proc-macro2+quote+syn. Adopt-minimal
  ([serde](https://raw.githubusercontent.com/serde-rs/serde/master/serde/Cargo.toml),
  [derive](https://raw.githubusercontent.com/serde-rs/serde/master/serde_derive/Cargo.toml)).
- **clap:** default = 11 nodes (anstream/strsim/utf8parse…); derive = 19
  via syn chain; granular std/color/help/usage/suggestions; no_std via
  no-default
  ([manifest](https://raw.githubusercontent.com/clap-rs/clap/master/clap_builder/Cargo.toml)).
  pico-args has zero `[dependencies]`, 4 trivial feats
  ([manifest](https://raw.githubusercontent.com/RazrFalcon/pico-args/master/Cargo.toml)).
  Replace with pico-args for simple CLIs; else builder-only
  no-default+std,help,usage. See doc 25 for the handroll/adopt boundary.
- **log:** zero-dep (1 node), core-only default with opt std/alloc/kv
  tiers. Adopt as-is; the facade for all libs
  ([manifest](https://raw.githubusercontent.com/rust-lang/log/master/Cargo.toml)).
  `tracing` is allowed in servers only when span correlation is needed
  (doc 23); its tree cost is unmeasured (doc 10).
- **anyhow:** zero-dep (1 node); default `[std]`; no_std via
  `default-features = false`. Adopt as-is for binaries; not for library
  API (erased errors)
  ([manifest](https://raw.githubusercontent.com/dtolnay/anyhow/master/Cargo.toml)).
- **cxx:** 13 nodes + cc build-dep + `links = cxxbridge1` (needs a C++
  toolchain); std/alloc tiers. Handroll extern-C by default; adopt-minimal
  only for true C++ APIs
  ([manifest](https://raw.githubusercontent.com/dtolnay/cxx/master/Cargo.toml)).
- **bindgen:** 27 nodes (syn-full, itertools, regex, nom, prettyplease,
  clang-sys) + libclang system dep (default runtime dlopen). Build-dep
  only; pre-generate + check in preferred; handroll or objc2 crates for
  Apple APIs
  ([manifest](https://raw.githubusercontent.com/rust-lang/rust-bindgen/master/bindgen/Cargo.toml)).
  See doc 27 for the handwritten-FFI-first policy.
- **ash:** 3 nodes (libloading+cfg-if); default `[loaded,debug,std]`;
  no_std via no-default+linked. Adopt-minimal no-default (+`loaded` or
  `linked`); macOS Vulkan needs MoltenVK
  ([manifest](https://raw.githubusercontent.com/ash-rs/ash/master/ash/Cargo.toml)).
- **wgpu:** 69 nodes even with no-default-feats (naga/wgpu-core/wgpu-hal
  floor); default adds dx12+metal+gles+vulkan+wgsl+webgpu
  ([manifest](https://raw.githubusercontent.com/gfx-rs/wgpu/master/wgpu/Cargo.toml)).
  objc2-metal deps are objc2+objc2-foundation+optional
  bitflags/block2/dispatch2/libc, no compiler dep
  ([manifest](https://raw.githubusercontent.com/madsmtm/objc2/master/framework-crates/objc2-metal/Cargo.toml)).
  Replace with objc2-metal on macOS; if wgpu is kept, no-default+
  metal,wgsl only.
- **objc2:** 2 nodes (objc2-encode zero-dep); std/alloc tiers;
  proc-macros/exception-helper optional. objc2-encode is zero-dep no_std
  with std/alloc tiers. Adopt-minimal, defaults fine; add
  exception/catch-all only if needed
  ([objc2](https://raw.githubusercontent.com/madsmtm/objc2/master/crates/objc2/Cargo.toml),
  [encode](https://raw.githubusercontent.com/madsmtm/objc2/master/crates/objc2-encode/Cargo.toml)).

## Per-item tiers

S: goblin|XOR|object, memmap2, nix, serde, clap→pico-args, log, anyhow,
ash, objc2. M: tokio→mio/threads swap, cxx→extern-C handroll,
bindgen build-time-only. L: wgpu→objc2-metal replacement.

## Gaps (→ doc 10)

MSRV/edition for all crates; full build+host tree (`cargo tree -e all`);
tracing tree cost; wgpu default-features total; object 0.40 vs 0.36 delta;
clap unicode/wrap_help incremental cost; bindgen without prettyplease;
cxx C++ compile-time cost.
