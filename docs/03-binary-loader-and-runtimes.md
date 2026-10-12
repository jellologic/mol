# 03 — Binary Loader and Runtimes

## Mach-O format

- Header `mach_header_64` (`MH_MAGIC_64=0xfeedfacf`, cputype, ncmds) plus load
  commands; loader parses headers, maps `LC_SEGMENT_64` segments, honors
  entry/dylinker commands:
  [loader.h](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/EXTERNAL_HEADERS/mach-o/loader.h).
- Must-handle load commands: `LC_SEGMENT_64`, `LC_MAIN`/`LC_UNIXTHREAD`,
  `LC_LOAD_DYLINKER`, `LC_DYLD_INFO(_ONLY)`, `LC_DYLD_CHAINED_FIXUPS`,
  `LC_DYLD_EXPORTS_TRIE`, `LC_UUID`, `LC_BUILD_VERSION`, `LC_CODE_SIGNATURE`,
  `LC_FILESET_ENTRY.
- Universal (fat) binaries wrap per-arch slices in big-endian
  `fat_header`/`fat_arch` (`FAT_MAGIC=0xcafebabe`); loader selects by
  cputype/cpusubtype:
  [fat.h](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/EXTERNAL_HEADERS/mach-o/fat.h).

## dyld

- dyld4 tracks each image with `Loader` objects (`PrebuiltLoader` for
  cached/closed images, `JustInTimeLoader` otherwise) over a shared
  `MachOAnalyzer` parser; replicate symbol binding, rebasing, chained fixups,
  initializer ordering: [dyld4.md](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/doc/dyld4.md).
- Shared cache merges system dylibs into TEXT/DATA/LINKEDIT mappings with
  page-sharing alignment; mol must either parse the cache or supply equivalent
  prebound dylibs: [CacheLayout.md](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/doc/CacheLayout.md).
- Reusable OSS: `common/MachOFile, MachOLoaded, MachOAnalyzer, MachOLayout,
  DyldSharedCache, ClosureFileSystem, CachePatching`:
  [listing](https://api.github.com/repos/apple-oss-distributions/dyld/contents/common?ref=main).

## ObjC runtime

- Dispatch via `objc_msgSend`/`objc_msgSendSuper` (+stret/fpret variants);
  implement messenger ABI incl. nil-receiver and struct/float returns:
  [message.h](https://raw.githubusercontent.com/apple-oss-distributions/objc4/main/runtime/message.h).
- Categories are `category_t` records attached at load; realize classes, then
  attach categories before `+load`:
  [objc-runtime-new.h](https://raw.githubusercontent.com/apple-oss-distributions/objc4/main/runtime/objc-runtime-new.h).
- `+load` order: loadable class/category lists, superclasses-first,
  parent-class-before-category — reproduce exactly:
  [objc-loadmethod.mm](https://raw.githubusercontent.com/apple-oss-distributions/objc4/main/runtime/objc-loadmethod.mm).
- ARC is compiler-inserted retain/release (`objc_retain/objc_release/
  objc_autorelease/objc_storeStrong/...`) plus autorelease pools; classes flag
  `RO_IS_ARC`: [Clang ARC](https://clang.llvm.org/docs/AutomaticReferenceCounting.html).

## Swift runtime

- ABI stable since Swift 5; runtime ships inside macOS — mol must provide
  Swift runtime dylibs with stable ABI (mangled names, value witness tables,
  metadata): [Swift 5](https://www.swift.org/blog/swift-5-released/),
  [manifesto](https://raw.githubusercontent.com/swiftlang/swift/main/docs/ABIStabilityManifesto.md).

## Code signing / entitlements (loader-visible only)

- Enforcement keys off `CS_*` flags (`CS_VALID/ADHOC/HARD/KILL/REQUIRE_LV/
  RUNTIME/PLATFORM_BINARY/LINKER_SIGNED`); Linux layer can skip verification
  but must parse `LC_CODE_SIGNATURE` blobs:
  [cs_blobs.h](https://raw.githubusercontent.com/apple-oss-distributions/xnu/main/osfmk/kern/cs_blobs.h).
- Reusable: `libsecurity_codesigning/lib/`:
  [listing](https://api.github.com/repos/apple-oss-distributions/libsecurity_codesigning/contents/lib?ref=main).
- AMFI (MAC hooks on load/mmap + per-page hashes) needs no Linux equivalent
  beyond optional signature parsing:
  [writeup](https://raw.githubusercontent.com/hack-different/apple-knowledge/main/_docs/Bypassing-AMFI.md).
- Entitlements are key-value grants in the signature; loader can ignore them
  except where behavior (JIT, library validation) is gated:
  [entitlements](https://developer.apple.com/documentation/bundleresources/entitlements.md),
  [hardened runtime](https://developer.apple.com/documentation/security/hardened-runtime.md).

## Tooling

- `goblin` crate: zero-copy Mach-O parser (header, load commands, segments,
  fat, bind opcodes, exports trie):
  [docs](https://docs.rs/goblin/latest/goblin/mach/index.html).
- `llvm-objdump --macho --bind --lazy-bind --rebase --chained-fixups
  --dyld-info --rpaths --universal-headers` is the reference inspector:
  [guide](https://llvm.org/docs/CommandGuide/llvm-objdump.html).

## Per-component tiers + Rust-versus-C

| Item | Tier | Lang |
|------|------|------|
| Mach-O/fat parse (goblin) | S | Rust |
| Segment map + slide (memmap2) | S–M | Rust |
| Bind/chained-fixups/exports-trie engine | L | Rust (port from dyld OSS logic) |
| Shared-cache parse vs prebound dylibs | M–L | Rust; decision open (doc 10) |
| dyld entry shim, TLS asm, commpage | M | C + asm, thin |
| ObjC messenger + category/`+load` order | L | Rust over objc2 where possible; C for ABI-critical paths |
| Swift runtime dylibs | L | Reuse Swift OSS drops; shim in Rust |
| LC_CODE_SIGNATURE parse (skip verify) | S–M | Rust |
