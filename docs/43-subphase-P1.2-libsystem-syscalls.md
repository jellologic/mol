# 43 — Sub-phase P1.2: libSystem Bring-up + Syscall Smoke

Parent: [31 — Phase P1](31-phase-P1-cli-core.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: exact init order; smoke-subset syscall table with Mac errnos;
libm ulp envelope.
Milestone: traced launch shows the exact init order; smoke syscalls
behave per Mac errno names.
Proven by: init-order trace test + conformance subset.

Criteria:

- [ ] P1.2.1 libSystem init order exact:
  `__libkernel_init` → `__libplatform_init` → `__pthread_init` →
  `_libc_initializer` → `__malloc_init` → `_dyld_initializer`,
  then `libdispatch_init` → `_libxpc_initializer` as documented
  stubs; full-order proof with real dispatch/XPC completes in P2.2
  (doc 12 L2).
- [ ] P1.2.2 Syscall smoke subset works: open/read/write/mmap/
  sigaction/posix_spawn with Mac errno names (docs 11 F1, 16 G2).
- [ ] P1.2.3 libm output within the measured Apple ulp envelope,
  not correctly-rounded (F3 T1, doc 11).

Tests:

- [ ] P1.2.V1 Per-module conformance tests in the Wine `dlls/*/tests`
  pattern; LTP-analog Darwin syscall subset (doc 16 G2).
- [ ] P1.2.V2 ulp-envelope measurement suite built from oracle
  traces; envelope checked in as the comparator (docs 11 F3, 14).
