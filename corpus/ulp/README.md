# V2 ulp envelope (doc 11 F3, P1.2.3)

The wrappers in `mol-libsystem/src/math.rs` forward to the host
libm. This directory pins how close that host libm stays to Apple
libm, measured rather than assumed.

- `apple-arm64.csv`: oracle trace. 11,690 rows
  (`func,in1,in2,out,errno`, bit-exact hex) from Apple libm on
  arm64 macOS: 25 edge inputs per function (zeros, subnormals,
  inf, NaN incl. signaling, int64 edges) plus 1,024 seeded
  pseudorandom bit patterns (2-arg functions cross the edges:
  625 + 1,024). Probe: `tools/ulp-oracle.c`
  (`clang -O2 -o /tmp/ulp-oracle tools/ulp-oracle.c`,
  then `/tmp/ulp-oracle > corpus/ulp/apple-arm64.csv`).
- `envelope.json`: per-function max ulp distance of the Linux
  host libm from the trace, plus one ulp headroom for libm
  version drift. Generated on Linux; re-run when the host libm
  changes:
  `cargo run -p mol-libsystem --example gen_envelope -- corpus/ulp/apple-arm64.csv corpus/ulp/envelope.json`.
- Comparator: `crates/mol-libsystem/tests/ulp.rs` (runs on both
  Linux arches in CI).

## Measured results (glibc 2.39 vs Apple arm64)

- Bit-identical: `sqrt`, `floor`, `ceil`, `fmod`, `fabs`
  (asserted exact, modulo NaN payloads).
- 0 ulps observed: `exp`, `log` (bound 1 with headroom).
- ≤1 ulp observed: `sin`, `cos`, `pow` (bound 2).
- `errno`: Apple sets it on zero of 11,690 samples
  (`math_errhandling == MATH_ERREXCEPT` only — verified
  `sqrt(-1)`, `log(-1)`, overflowing `exp`). The wrappers
  therefore leave the guest cell strictly untouched; the
  comparator asserts a sentinel survives every call.

## Fixes the oracle forced

- `pow(1.0, sNaN)`: Apple returns 1.0 (C99 short-circuit
  first); glibc returns NaN. `mol_pow` now applies
  `pow(1, y) == pow(x, ±0) == 1` before calling the host.
  (`pow(-1, non-integer)` stays NaN — domain error, per trace.)
- Dropped the old glibc-errno propagation: it set `EDOM`
  where Apple leaves errno alone.

## Scope notes

- NaN payloads are out of scope (asserted NaN-ness only).
- The oracle is arm64 Apple libm; Intel-Apple libm is
  unmeasured (no Intel Mac on hand). Both Linux arches
  measured identical bounds against this trace.
