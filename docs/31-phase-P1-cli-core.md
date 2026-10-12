# 31 — Phase P1: Same-ISA CLI Core (→M1, gates G1+G2, layers L0–L4)

Parent: [29 — Implementation plan](29-implementation-plan.md).
Entry: P0 exit milestone green (harness gates merges).

Objective: macOS CLI binaries load and run, AMD64 first then ARM64.

Requirements: thin+fat Mach-O parse; segment map + slide; chained
fixups + legacy bind opcodes; dyld load→fixup→init order with
closure policy; exact libSystem init order; BSD syscall table
starting from the smoke subset; libm ulp envelope; guest
codesign/entitlement policy; Tier-1 linkage closure; ObjC
`_objc_init` + `+load` ordering + messenger ABI; Swift runtime
dylibs loadable; minimal mol-server with bootstrap registry;
launchd-as-init in the prefix.

Sub-phases (in order):

- [P1.1 — Mach-O loader + dyld engine](42-subphase-P1.1-loader-dyld.md):
  `LC_MAIN` reached, zero-diff load maps on thin+fat corpus.
- [P1.2 — libSystem bring-up + syscall smoke](43-subphase-P1.2-libsystem-syscalls.md):
  exact init order traced; smoke syscalls Mac-correct.
- [P1.3 — ObjC + Swift runtimes](44-subphase-P1.3-objc-swift.md):
  `+load` ordering green; Swift dylibs load.
- [P1.4 — Server skeleton + Tier-1 exit](45-subphase-P1.4-server-tier1-exit.md):
  Tier 1 runs byte-identical on both arches.

Exit milestone: macOS jq, curl, ffmpeg run with byte-identical
stdout vs macOS on both arches — non-TLS surface (TLS in P3.2)
(P1.4 green, Tier 1). Proven by: corpus board + differential
diff job.
