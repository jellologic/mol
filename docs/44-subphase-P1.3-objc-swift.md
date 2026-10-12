# 44 — Sub-phase P1.3: ObjC + Swift Runtimes

Parent: [31 — Phase P1](31-phase-P1-cli-core.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: `_objc_init`, `+load` ordering, messenger ABI, Swift dylibs
loadable.
Milestone: `+load` ordering fixture green; Swift runtime dylibs
load and initialize.
Proven by: ordering fixture + Swift load test.

Criteria:

- [ ] P1.3.1 ObjC ordering fixture green: superclass-first class
  `+load`s, category `+load`s after parent, cross-image categories
  (doc 12 L4).
- [ ] P1.3.2 Swift runtime dylibs load and initialize (doc 09 M1).
- [ ] P1.3.3 `objc_msgSend` ABI differential-clean incl nil-receiver,
  stret/fpret variants, and ARC entry points (`objc_retain` /
  `objc_release` / autorelease pools) (doc 03).

Tests:

- [ ] P1.3.V1 Ordering fixture covers deep hierarchies +
  cross-image categories (doc 12 L4).
