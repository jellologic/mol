# 12 — Foundation Build Order

Bottom-up bring-up order L0→L6. Each layer lists its done-criteria and
the evidence it rests on. Do not start a layer until the previous
layer's checklist is green — init-order bugs are the costliest class
here.

## L0 — Prefix + overlay + namespaces (Tier: M)

- `unshare --mount --pid --fork` gives a private mount table plus a PID1
  init; set propagation private:
  [unshare.1](https://man7.org/linux/man-pages/man1/unshare.1.html).
- Overlay mount `-t overlay -o lowerdir,upperdir,workdir`; workdir must
  share a fs with upperdir; omit upper/work for a read-only stack:
  [overlayfs](https://docs.kernel.org/filesystems/overlayfs.html).
- PID ns: first child is PID1; if init dies the kernel SIGKILLs the ns
  and fork fails ENOMEM; only handled signals reach init:
  [pid_namespaces.7](https://man7.org/linux/man-pages/man7/pid_namespaces.7.html).
- Mount ns copies caller (unshare) or parent (clone `CLONE_NEWNS`)
  mounts; later mounts stay private modulo shared subtrees:
  [mount_namespaces.7](https://man7.org/linux/man-pages/man7/mount_namespaces.7.html).
- Dyld `CacheFinder` checks `DYLD_SHARED_CACHE_DIR` first, then
  ignition/cryptex, then system dir — the compat prefix sets the
  override to the prefix cache file:
  [DyldProcessConfig.cpp](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/dyld/DyldProcessConfig.cpp).
- Tension (open): determinism controls want a fixed sysroot +
  `DYLD_*` overrides, but PrebuiltLoaderSet policy says closures are
  never used with overrides / are invalid after reboot (see L3). The
  harness must pick one cache mode per run and record it.

Done when: merged root boots PID1; `DYLD_SHARED_CACHE_DIR` points at
the prefix cache.

## L1 — Mach-O loader: fat/segment/slide/fixups/entry (Tier: L)

- Fat: `FatFile::isFatFile` then `forEachSlice` validates slice by
  cputype/cpusubtype before Mach-O parse:
  [MachOFile.h](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/common/MachOFile.h).
- Segments: `LC_SEGMENT_64` maps file ranges; `MachOLoaded` models the
  mapped image with zero-fill; `getSlide` reports the ASLR slide:
  [MachOLoaded.h](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/common/MachOLoaded.h).
- Chained fixups: `LC_DYLD_CHAINED_FIXUPS` payload
  `dyld_chained_fixups_header` with starts/imports/symbols
  offsets+counts; per-segment page-start chains:
  [fixup-chains.h](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/include/mach-o/fixup-chains.h).
- Legacy binds: `LC_DYLD_INFO_ONLY` bytecodes
  `SET_SEGMENT_AND_OFFSET_ULEB`/`DO_BIND*`/`DONE` plus threaded `0xD0`;
  `LC_MAIN.entryoff` points at main:
  [loader.h](https://raw.githubusercontent.com/apple-oss-distributions/cctools/main/include/mach-o/loader.h).
- Bring-up: dyld `prepare()` does loadDependents → applyFixups →
  runAllInitializersForMain, then `getEntry` jumps to `LC_MAIN`:
  [dyldMain.cpp](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/dyld/dyldMain.cpp).

Done when: `LC_MAIN` reached on thin+fat corpus with zero-diff load
maps vs `otool -l`/`dyld_info`.

## L2 — libSystem_initializer call order (Tier: M)

Exact order (reproduce it):
`__libkernel_init` → `__libplatform_init` → `__pthread_init` →
`_libc_initializer` → `__malloc_init` → `_dyld_initializer` →
`libdispatch_init` → `_libxpc_initializer` → trace/secinit/container/darwin:
[init.c](https://raw.githubusercontent.com/apple-open-source-mirror/Libsystem/master/init.c).

Done when: errno zeroed, atfork chain set, each init observed in order
in a traced launch.

## L3 — PrebuiltLoaderSet / closures (Tier: M)

- dyld4: per-image `PrebuiltLoader` (read-only, cache-built) or
  `JustInTimeLoader` (malloced, parses on the fly); the Loader array
  lives in `DyldRuntimeState`:
  [dyld4.md](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/doc/dyld4.md).
- Policy: per-app sets saved under
  `~/Library/Caches/com.apple.dyld/`; never without cache / with
  interposing / with `DYLD_*` overrides; `DYLD_USE_CLOSURES` forces;
  invalid after reboot:
  [PrebuiltLoaderSet_Policy.md](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/doc/PrebuiltLoaderSet_Policy.md).
- Open: literal default cache-dir constant
  (`MACOSX_MRM_DYLD_SHARED_CACHE_DIR`, expected
  `/System/Library/dyld`) — macro site not found; placement via the
  `DYLD_SHARED_CACHE_DIR` override is verified instead.

Done when: cached launch skips JIT parse; override/reboot
invalidation behaves per policy.

## L4 — _objc_init + load_images before any +load (Tier: L)

- `_objc_init` runs locks/environ/tls/sync/accessors/sidetables/static/
  runtime/cache init, then
  `_dyld_objc_register_callbacks(map_images, load_images, unmap)`:
  [objc-os.mm](https://raw.githubusercontent.com/apple-oss-distributions/objc4/main/runtime/objc-os.mm).
- `+load` guarantee: `call_load_methods` drains class `+load`s
  superclass-first, then category `+load`s once after the parent
  class's `+load`; re-entrant-safe, outermost-only:
  [objc-loadmethod.mm](https://raw.githubusercontent.com/apple-oss-distributions/objc4/main/runtime/objc-loadmethod.mm).

Done when: superclass-first / category-after ordering holds on a
fixture with deep hierarchies + cross-image categories.

## L5 — libdispatch, then libxpc; launchd on-demand services (Tier: M)

- `libdispatch_init` (from `libSystem_initializer`, after dyld init)
  creates TSD keys, binds the main queue, sets pthread_atfork
  handlers, inits hw/time/vtable/os_object/voucher; root queues come
  up lazily via `_pthread_workqueue`:
  [queue.c](https://raw.githubusercontent.com/apple/swift-corelibs-libdispatch/main/src/queue.c).
- XPC service: launchd launches on demand, restarts on crash, SIGKILLs
  when idle; services are stateless, private to the containing app,
  each with its own sandbox:
  [XPC services](https://developer.apple.com/library/archive/documentation/MacOSX/Conceptual/BPSystemStartup/Chapters/CreatingXPCServices.html).
- XPC listener: the launchd job must call
  `xpc_connection_create_mach_service` with
  `XPC_CONNECTION_MACH_SERVICE_LISTENER`; the name must appear in the
  launchd.plist `MachServices` dict:
  [xpc man](https://www.unix.com/man-page/osx/3/xpc_connection_set_event_handler/).
- Open: `_libxpc_initializer` internals (bootstrap port lookup order) —
  libxpc source is unpublished; bring-up is specified at the
  launchd/XPC API level only.

Done when: main queue + a MachService listener are live; on-demand
launch / crash-restart / idle-kill cycle passes fault injection.

## L6 — __CFInitialize, then Foundation, then AppKit (Tier: L–XL)

- `__CFInitialize` constructor (once-guarded) records the main
  pthread, TSD, env, registers the `__kCF*` runtime classes; allocator
  init precedes any CF use:
  [CFRuntime.c](https://raw.githubusercontent.com/apple-oss-distributions/CF/main/CFRuntime.c).
- Layering: Foundation is OO CoreFoundation in Core Services; AppKit
  depends on Foundation; CLI tools need only Foundation, GUI apps add
  AppKit:
  [WhatIsCocoa](https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CocoaFundamentals/WhatIsCocoa/WhatIsCocoa.html).
- Open: Foundation/AppKit internal bring-up (`NSApplicationMain`
  sequence) is closed-source and current developer.apple.com pages are
  JS shells; specify from behavior + golden traces instead.

Done when: CF classes registered, NS layer usable (Tier 1 CLI
exercises it), AppKit session starts (Tier 3).

## Master checklist

- [ ] L0: prefix boots PID1, cache override honored
- [ ] L1: `LC_MAIN` reached, load maps zero-diff
- [ ] L2: init order exact, errno/atfork verified
- [ ] L3: closure fast-path + invalidation per policy
- [ ] L4: `+load` ordering fixture green
- [ ] L5: GCD + XPC + launchd lifecycle green
- [ ] L6: CF → Foundation → AppKit staged bring-up green
