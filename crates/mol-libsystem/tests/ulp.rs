// SPDX-License-Identifier: MIT OR Apache-2.0
//! V2 ulp-envelope comparator (doc 11 F3, P1.2.3): evaluates our
//! libm wrappers over the checked-in Apple oracle trace and asserts
//! exact-set bitwise identity plus envelope bounds for the rest.
//! Linux-only (it measures the Linux host libm); the envelope pins
//! the measured libm — re-run `gen_envelope` when it changes.
#![cfg(target_os = "linux")]

use mol_libsystem::math;
use mol_libsystem::ulp::{is_nan_bits, parse_envelope, parse_trace, ulp_dist, EXACT_FNS};
use std::path::{Path, PathBuf};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap()
        .join("corpus")
        .join("ulp")
}

fn call(func: &str, in1: u64, in2: Option<u64>) -> f64 {
    let x = f64::from_bits(in1);
    match func {
        "sin" => math::mol_sin(x),
        "cos" => math::mol_cos(x),
        "exp" => math::mol_exp(x),
        "log" => math::mol_log(x),
        "sqrt" => math::mol_sqrt(x),
        "floor" => math::mol_floor(x),
        "ceil" => math::mol_ceil(x),
        "fabs" => math::mol_fabs(x),
        "pow" => math::mol_pow(x, f64::from_bits(in2.unwrap())),
        "fmod" => math::mol_fmod(x, f64::from_bits(in2.unwrap())),
        f => panic!("unknown func {f}"),
    }
}

#[test]
fn within_envelope_and_errno_untouched() {
    let dir = corpus();
    let rows = parse_trace(&dir.join("apple-arm64.csv")).expect("parse trace");
    assert!(rows.len() > 10_000, "trace too small: {}", rows.len());
    let env = parse_envelope(&dir.join("envelope.json")).expect("parse envelope");
    mol_syscall::errcell::set(43);
    for r in &rows {
        let mb = call(&r.func, r.in1, r.in2).to_bits();
        // Oracle: Apple leaves errno 0 on every sample.
        assert_eq!(r.errno, 0, "{} oracle errno drift", r.func);
        assert_eq!(
            mol_syscall::errcell::get(),
            43,
            "{} touched guest errno",
            r.func
        );
        if is_nan_bits(r.out) {
            assert!(is_nan_bits(mb), "{}: oracle NaN, ours {mb:x}", r.func);
            continue;
        }
        assert!(!is_nan_bits(mb), "{}: ours NaN, oracle {:x}", r.func, r.out);
        if EXACT_FNS.contains(&r.func.as_str()) {
            assert_eq!(
                mb, r.out,
                "{}: exact-set mismatch in {:x}: ours {mb:x}, oracle {:x}",
                r.func, r.in1, r.out
            );
            continue;
        }
        let bound = env
            .get(&r.func)
            .unwrap_or_else(|| panic!("no envelope for {}", r.func));
        let d = ulp_dist(mb, r.out);
        assert!(
            d <= *bound,
            "{}: {d} ulps over bound {bound} (in {:x}/{:x?}, ours {mb:x}, oracle {:x})",
            r.func,
            r.in1,
            r.in2,
            r.out
        );
    }
}
