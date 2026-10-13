# 44 — Sub-phase P1.3: ObjC + Swift Runtimes

Parent: [31 — Phase P1](31-phase-P1-cli-core.md) ·
Plan: [29 — Implementation plan](29-implementation-plan.md).

Scope: `_objc_init`, `+load` ordering, messenger ABI, Swift dylibs
loadable.
Milestone: `+load` ordering fixture green; Swift runtime dylibs
load and initialize.
Proven by: ordering fixture + Swift load test.

Criteria:

- [x] P1.3.1 ObjC ordering fixture green: superclass-first class
  `+load`s, category `+load`s after parent, cross-image categories
  (doc 12 L4). (`mol-objc` schedule/drain per `objc-loadmethod.mm`;
  loader drains mapped-memory parses between inits and entry via
  the `call_imp` trampoline.)
- [x] P1.3.2 Swift runtime dylibs load and initialize (doc 09 M1).
  (Swift-shaped dylib: `__swift5_proto` + Swift class `+load` +
  `LC_ROUTINES_64` init + mangled export; `swift_dylib` E2E
  asserts exact `SWFE` stdout + exit 0 on both arches. Fixed a
  test-emitter bug along the way: `exports_trie` omitted the
  leaf child-count byte, which single-export tries expose as
  `Truncated { need: 25, len: 24 }`.)
- [ ] P1.3.3 `objc_msgSend` ABI differential-clean incl nil-receiver,
  stret/fpret variants, and ARC entry points (`objc_retain` /
  `objc_release` / autorelease pools) (doc 03).

Tests:

- [x] P1.3.V1 Ordering fixture covers deep hierarchies +
  cross-image categories (doc 12 L4). (5 byte-level ordering
  fixtures with exact `(imp, cls, sel)` sequences + `objc_guest`
  E2E asserting `RMCE` on both arches.)
