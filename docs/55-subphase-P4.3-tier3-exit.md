# 55 — Sub-phase P4.3: Tier-3 Exit Apps

Parent: [34 — Phase P4](34-phase-P4-gui-shell.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: exit apps run unaided.
Milestone: Calculator-clone + TextEdit-clone usable; one OSS Cocoa
app runs; all on both arches.
Proven by: unaided scripted sessions + sanitizer-clean suites.

Criteria:

- [ ] P4.3.1 Exit: Calculator-clone and TextEdit-clone (authored test
  apps) usable; one OSS Cocoa app runs; all on both arches. Genuine
  Apple-signed system apps remain out of scope per doc 01
  (doc 09 M4 Tier 3).

Tests:

- [ ] P4.3.V1 Example apps launch unaided, survive the scripted
  session, exit clean (doc 16 G5 DoD).
- [ ] P4.3.V2 ASan/UBSan + Valgrind clean on the GUI suites
  (doc 16 G7).
