# 06 — Cross-Arch Strategy

## Approach matrix

| # | Guest → Host | Approach | Cost |
|---|--------------|----------|------|
| 1 | arm64-macOS → ARM64-Linux | Darling-style Mach-O loader + Darwin/Mach syscall server, **no CPU emulation**; page size benign (16K guest assumptions hold on 4K host; audit reverse); strip or NOP arm64e PAC | Lowest ARM case |
| 2 | x86_64-macOS → AMD64-Linux | Same-ISA Darling approach; **simplest case**, no TSO/page issues (both 4K TSO) | Simplest |
| 3 | x86_64-macOS → ARM64-Linux | FEX-style IR JIT or Box64-style dynarec + wrapping for CPU **plus** Darling OS layer; heaviest cost is x86-TSO emulation (barriers/LRCPC, unaligned atomics, split-locks); Rosetta lesson: whole-process AOT at install + JIT fallback, never mix ISAs in one process | Hardest |
| 4 | arm64-macOS → AMD64-Linux | QEMU-TCG-class generic translator + Darling loader (Box64/FEX do not do ARM-on-x86); memory model free since TSO host satisfies ARM guest | Hard |

Sources: Darling same-ISA model
([README](https://raw.githubusercontent.com/darlinghq/darling/master/README.md)),
FEX ([site](https://fex-emu.com/), [README](https://raw.githubusercontent.com/FEX-Emu/FEX/main/Readme.md)),
Box64 ([dynarec](https://box86.org/2021/07/inner-workings-a-high‑level-view-of-box86-and-a-low‑level-view-of-the-dynarec/),
[wrapping](https://box86.org/2021/08/a-deep-dive-into-library-wrapping/),
[README](https://raw.githubusercontent.com/ptitSeb/box64/main/README.md)),
QEMU ([user](https://www.qemu.org/docs/master/user/main.html),
[TCG](https://www.qemu.org/docs/master/devel/tcg.html)),
Rosetta ([Apple](https://developer.apple.com/documentation/apple-silicon/about-the-rosetta-translation-environment.md)).

## Memory-model notes (matrix #3 driver)

- x86-TSO vs ARM weak ordering are opposite extremes; ARM needs LRCPC/RCpc,
  acquire/release, LSE2; split-locks stay slow even on M1:
  [FEX analysis](https://fex-emu.com/Scourge-of-emulation/).
- Apple Silicon M1 implements x86-TSO in hardware, enabled per-process for
  Rosetta; XNU defines `ACTLR_EL1_EnTSO` bit 1:
  [proc_reg.h](https://raw.githubusercontent.com/apple/darwin-xnu/a1babec6b135d1f35b2590a1990af3c5c5393479/osfmk/arm64/proc_reg.h).
- QEMU warns host/guest memory-model differences break software assuming a
  model: [user docs](https://www.qemu.org/docs/master/user/main.html).
- FEX needs an x86_64 RootFS; per-app config can skip costly memory-model
  emulation.

## Page-size notes

- Apple Silicon runs native processes with 16K pages but Rosetta x86_64
  processes with 4K pages; per-4K `mprotect` is inexpressible on a 16K host:
  [research](https://raw.githubusercontent.com/shalaga44/rosetta-4k-pages/HEAD/docs/research.md).
- XNU ships parallel 4K/16K VM code variants (`WKdmCompress_16k.s` vs `_4k.s`
  in `osfmk/arm64`):
  [listing](https://api.github.com/repos/apple/darwin-xnu/contents/osfmk/arm64).
- Apple tells developers page sizes differ vs Intel Macs — fetch
  `vm_page_size` dynamically, never hardcode:
  [Apple doc](https://developer.apple.com/documentation/apple-silicon/addressing-architectural-differences-in-your-macos-code.md).
- Open: distro survey of default ARM64 page size; whether x86_64 macOS
  binaries assume 4K mprotect granularity (see doc 10).

## Pointer authentication (PAC)

- Linux ARM64 PAC: 5 keys, PAC in high pointer bits, HINT-space NOPs without
  HW so PAC binaries still run unprotected; random per-process keys at exec:
  [kernel docs](https://docs.kernel.org/arch/arm64/pointer-authentication.html).
- Apple arm64e signs unused high pointer bits; auth failure poisons the
  pointer; needs A12+/Apple Silicon:
  [Apple doc](https://developer.apple.com/documentation/security/preparing-your-app-to-work-with-pointer-authentication.md).
- Guidance: plain arm64 macOS binaries are unaffected; arm64e slices run as
  NOPs on non-PAC Linux ARM64, need key-compat audit on PAC HW. PAC is
  otherwise transparent.

## Cross-cutting rules

1. Copy Wine's PE-vs-Unix split: guest ABI surface separate from host syscall
   backend, with a central ID/handle server.
2. Prefer wrapped/thunked native libs (Box64/FEX style) over emulating
   libc/GL/Vulkan.
3. Rosetta rule: whole-process translation, never mixed ISAs in one process.

## Tiers

| Item | Tier |
|------|------|
| Matrix #2 (x86_64→AMD64), #1 (arm64→ARM64) | M–L (loader + server only) |
| Matrix #4 (arm64→AMD64, QEMU-TCG class) | XL |
| Matrix #3 (x86_64→ARM64, IR JIT/dynarec + TSO) | XL, last |
