# 02 — Prior Art

## Darling (primary model)

- FOSS macOS compatibility layer for Linux, "WINE counterpart":
  [Wikipedia](https://en.wikipedia.org/wiki/Darling_(software)),
  [darlinghq/darling](https://github.com/darlinghq/darling),
  [darlinghq.org](https://www.darlinghq.org/).
- Started summer 2012 from maloader; CLI works, Cocoa GUI basic, prefixes use
  overlayfs + namespaces.
- Since 2017: Mach-O loader launches Apple dyld; XNU/Mach IPC via userspace
  darlingserver (old kernel module dropped as too unstable and hard to debug).
- darlingserver is wineserver-like, embedding duct-taped XNU code on
  cooperative microthreads/fibers (not one Linux thread per guest thread),
  RPC over per-thread Unix datagram sockets with SCM_RIGHTS FD passing.
  Sources: [system-call-emulation](https://github.com/darlinghq/darling-docs/blob/master/src/internals/basics/system-call-emulation.md),
  [server README](https://github.com/darlinghq/darling-docs/blob/master/src/internals/darlingserver/README.md),
  [RPC](https://github.com/darlinghq/darling-docs/blob/master/src/internals/darlingserver/rpc.md),
  [duct-tape](https://github.com/darlinghq/darling-docs/blob/master/src/internals/darlingserver/duct-tape.md).
- Userland: ~149 submodule forks of Apple open source (Libc, dyld, objc4,
  ICU, shells) plus reimplemented frameworks (Cocotron AppKit, Apportable
  Foundation, CFLite CoreFoundation, Metal-on-Vulkan in progress).
- License: top-level GPL-3.0; Apple-file submodules add APSL headers, Cocotron
  MIT, Foundation LGPL-2.1. See doc 08.
- What works: Homebrew, Xcode CLI tools, CMake, Emacs, Python 3.11,
  Gnuplot-to-PNG
  ([list](https://github.com/darlinghq/darling-docs/blob/master/src/known-working-software.md)).
- Broken: all but hello-world GUI, Xcode GUI, Logic, FCP, Adobe, Catalyst (no
  UIKit), XQuartz hangs, MacPorts
  ([list](https://github.com/darlinghq/darling-docs/blob/master/src/known-nonfunctional-software.md)).
- Partial: Classic Finder (no menubar, focus bugs, slow), dotnet hello-world
  only ([list](https://github.com/darlinghq/darling-docs/blob/master/src/known-partial-software.md)).

## Wine / Proton (server + PE/Unix-split lessons)

- Central `wineserver` owns Win32 object/handle semantics via a generated RPC
  protocol: [protocol.def](https://gitlab.winehq.org/wine/wine/-/raw/master/server/protocol.def),
  NTDLL Unix side over Unix-domain socket:
  [server.c](https://gitlab.winehq.org/wine/wine/-/raw/master/dlls/ntdll/unix/server.c).
- Each DLL splits into PE side + Unix side (`#pragma makedep unix`):
  [loader.c](https://gitlab.winehq.org/wine/wine/-/raw/master/dlls/ntdll/unix/loader.c).
  Lesson for mol: keep the guest ABI surface separate from the host syscall
  backend, with one central ID/handle server.
- [Proton](https://raw.githubusercontent.com/ValveSoftware/Proton/proton_10/README.md):
  Wine-based, SDK-container builds — precedent for containerized compat builds.

## Box64 / FEX / QEMU / Rosetta (emulation lessons)

- Box64: dynarec with flag-analysis passes; wrapped native libs via bridge
  trampolines exiting emulation ([dynarec](https://box86.org/2021/07/inner-workings-a-high‑level-view-of-box86-and-a-low‑level-view-of-the-dynarec/),
  [wrapping](https://box86.org/2021/08/a-deep-dive-into-library-wrapping/),
  [README](https://raw.githubusercontent.com/ptitSeb/box64/main/README.md)).
- FEX: IR-based x86→ARM64 JIT with syscall translation and GL/Vulkan
  forwarding thunks ([site](https://fex-emu.com/),
  [README](https://raw.githubusercontent.com/FEX-Emu/FEX/main/Readme.md)).
- QEMU user-mode + TCG: syscall translation, signal synthesis, translation
  blocks with chaining ([user](https://www.qemu.org/docs/master/user/main.html),
  [TCG](https://www.qemu.org/docs/master/devel/tcg.html)).
- Rosetta: whole-process AOT at install + JIT fallback, never mixed ISAs in
  one process; no kexts/VM apps; AVX1/2 yes, AVX512 no
  ([Apple doc](https://developer.apple.com/documentation/apple-silicon/about-the-rosetta-translation-environment.md)).

## Lessons / pitfalls for mol

1. Emulate at the libSystem boundary; accept that direct-syscall binaries
   (old UPX packings, some Go) cannot run.
2. Budget most effort for Mach IPC semantics and ObjC/frameworks, not the loader.
3. Reuse Apple OSS (XNU/libc/dyld) rather than clean-rooming syscalls.
4. Isolate kernel-facing shims (commpage fixed address, TLS setup,
   signal/exception delivery) per-arch.
5. Generate framework stubs from a real Mac (needs SDK/class-dump access).
6. Avoid a kernel module (instability, debug cost, distro friction).
7. Expect GUI to dominate schedule (Darling: 10+ years, still hello-world-only).
8. Overlayfs prefixes break on NFS/eCryptfs/encrypted homes — plan fallback.
9. Pin contracts in tests; docs rot fast (e.g. `src/kernel` → `darling-xnu` move).
