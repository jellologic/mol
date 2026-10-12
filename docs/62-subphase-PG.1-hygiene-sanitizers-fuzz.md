# 62 — Sub-phase PG.1: Binary Hygiene + Sanitizers + Fuzz

Parent: [37 — Phase PG](37-phase-PG-hardening.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: signing, sanitizers, fuzz-zero.
Milestone: binaries verify, suites sanitizer-clean, fuzzers quiet.
Proven by: make-check-analog + sanitizer leg.

Criteria:

- [ ] PG.1.1 `codesign -v` clean on all shipped binaries incl fat;
  entitlements/sandbox profiles reviewed (doc 16 G7).
- [ ] PG.1.2 Valgrind leak-clean + ASan/UBSan clean on the CLI (+GUI
  from P4) suites (doc 16 G7).
- [ ] PG.1.3 Fuzz crashers at zero for loader/IPC/image parsers over
  the budgeted corpus before release (doc 16 G7).

Tests:

- [ ] PG.1.V1 `make check`-analog green (unit + integration +
  golden-trace equivalents, doc 16 G7).
- [ ] PG.1.V2 Sanitizer CI leg green; device-crash-matrix analog
  green.
