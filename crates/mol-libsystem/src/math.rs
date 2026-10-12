// SPDX-License-Identifier: MIT OR Apache-2.0
//! libm wrappers through the host `libm` (doc 11 F3, P1.2.3).
//!
//! Guests call the real `libm` entry points (`_sin`, `_pow`, …);
//! mol forwards to the host math library. The guest errno cell is
//! left strictly untouched: Apple's `math_errhandling` is
//! `MATH_ERREXCEPT` only (verified by the V2 oracle probe —
//! `sqrt(-1)`, `log(-1)`, overflowing `exp` all leave errno 0), so
//! propagating glibc's `EDOM`/`ERANGE` would diverge. Fidelity is
//! the measured ulp envelope (V2), not correct rounding: Apple
//! `cosf` itself is not correctly rounded.
//!
//! Linux-only: guests execute on Linux. One system-library edge
//! (`-lm`) under doc 27; no Rust dependencies.

#[link(name = "m")]
unsafe extern "C" {
    fn sin(x: f64) -> f64;
    fn cos(x: f64) -> f64;
    fn exp(x: f64) -> f64;
    fn log(x: f64) -> f64;
    fn pow(x: f64, y: f64) -> f64;
    fn sqrt(x: f64) -> f64;
    fn floor(x: f64) -> f64;
    fn ceil(x: f64) -> f64;
    fn fmod(x: f64, y: f64) -> f64;
    fn fabs(x: f64) -> f64;
}

/// Forward to the host function, leaving errno (host and guest)
/// exactly as found.
macro_rules! wrap1 {
    ($mol:ident, $host:ident) => {
        pub extern "C" fn $mol(x: f64) -> f64 {
            // SAFETY: pure arithmetic on a finite-or-not double.
            unsafe { $host(x) }
        }
    };
}

macro_rules! wrap2 {
    ($mol:ident, $host:ident) => {
        pub extern "C" fn $mol(x: f64, y: f64) -> f64 {
            // SAFETY: as above.
            unsafe { $host(x, y) }
        }
    };
}

wrap1!(mol_sin, sin);
wrap1!(mol_cos, cos);
wrap1!(mol_exp, exp);
wrap1!(mol_log, log);
/// `pow` with the C99-mandated short-circuits first: `pow(1, y)`
/// and `pow(x, ±0)` are exactly 1 for any other operand, even NaN.
/// The host libm gets the order wrong for signaling NaNs (glibc
/// `pow(1.0, sNaN)` is NaN; Apple returns 1.0 — oracle trace).
/// Note `pow(-1, non-integer)` is NaN (domain error), not 1.
pub extern "C" fn mol_pow(x: f64, y: f64) -> f64 {
    if x == 1.0 || y == 0.0 {
        return 1.0;
    }
    // SAFETY: pure arithmetic on finite-or-not doubles.
    unsafe { pow(x, y) }
}
wrap1!(mol_sqrt, sqrt);
wrap1!(mol_floor, floor);
wrap1!(mol_ceil, ceil);
wrap2!(mol_fmod, fmod);
wrap1!(mol_fabs, fabs);

/// Handler address for a math export (names in
/// [`crate::MATH_EXPORTS`]).
pub fn export_addr(name: &str) -> Option<u64> {
    Some(match name {
        "_sin" => mol_sin as *const () as u64,
        "_cos" => mol_cos as *const () as u64,
        "_exp" => mol_exp as *const () as u64,
        "_log" => mol_log as *const () as u64,
        "_pow" => mol_pow as *const () as u64,
        "_sqrt" => mol_sqrt as *const () as u64,
        "_floor" => mol_floor as *const () as u64,
        "_ceil" => mol_ceil as *const () as u64,
        "_fmod" => mol_fmod as *const () as u64,
        "_fabs" => mol_fabs as *const () as u64,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spot_values_sane() {
        assert_eq!(mol_sin(0.0), 0.0);
        assert_eq!(mol_cos(0.0), 1.0);
        assert_eq!(mol_exp(0.0), 1.0);
        assert_eq!(mol_sqrt(4.0), 2.0);
        assert_eq!(mol_floor(1.5), 1.0);
        assert_eq!(mol_ceil(1.5), 2.0);
        assert_eq!(mol_fabs(-2.5), 2.5);
        assert_eq!(mol_pow(2.0, 10.0), 1024.0);
        assert_eq!(mol_fmod(5.5, 2.0), 1.5);
        assert!(mol_log(1.0) == 0.0);
    }

    #[test]
    fn pow_special_cases_match_c99() {
        let qnan = f64::from_bits(0x7ff8_0000_0000_0000);
        let snan = f64::from_bits(0x7ff4_0000_0000_0000);
        assert_eq!(mol_pow(1.0, qnan), 1.0);
        assert_eq!(mol_pow(1.0, snan), 1.0);
        assert_eq!(mol_pow(-1.0, f64::INFINITY), 1.0);
        assert!(mol_pow(-1.0, 0.5).is_nan());
        assert_eq!(mol_pow(qnan, 0.0), 1.0);
        assert_eq!(mol_pow(qnan, -0.0), 1.0);
    }

    #[test]
    fn domain_error_leaves_guest_errno() {
        // Oracle: Apple sqrt(-1) returns NaN with errno untouched.
        mol_syscall::errcell::set(7);
        let r = mol_sqrt(-1.0);
        assert!(r.is_nan());
        assert_eq!(mol_syscall::errcell::get(), 7);
        mol_syscall::errcell::set(0);
    }
}
