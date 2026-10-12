// SPDX-License-Identifier: MIT OR Apache-2.0
//! libm wrappers through the host `libm` (doc 11 F3, P1.2.3).
//!
//! Guests call the real `libm` entry points (`_sin`, `_pow`, …);
//! mol forwards to the host math library and propagates domain
//! errors (`EDOM`/`ERANGE`, same numbers both OSes) into the guest
//! errno cell. Fidelity is the measured ulp envelope (V2), not
//! correct rounding: Apple `cosf` itself is not correctly rounded.
//!
//! Linux-only: guests execute on Linux. One system-library edge
//! (`-lm` + `__errno_location` from `-lc`) under doc 27; no Rust
//! dependencies.

use core::ffi::c_int;

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

// glibc and musl both provide `__errno_location` (the Ubuntu
// Desktop target is glibc).
#[link(name = "c")]
unsafe extern "C" {
    fn __errno_location() -> *mut c_int;
}

/// Call a host libm function with the zero-check-propagate errno
/// protocol: clear the HOST cell first (Rust std may have left it
/// stale), call, propagate a fresh nonzero errno to the GUEST cell.
macro_rules! wrap1 {
    ($mol:ident, $host:ident) => {
        pub extern "C" fn $mol(x: f64) -> f64 {
            // SAFETY: pure arithmetic on a finite-or-not double; the
            // errno cell pointer is valid for this thread.
            unsafe {
                *__errno_location() = 0;
                let r = $host(x);
                let e = *__errno_location();
                if e != 0 {
                    mol_syscall::errcell::set(mol_syscall::errno::from_linux(e));
                }
                r
            }
        }
    };
}

macro_rules! wrap2 {
    ($mol:ident, $host:ident) => {
        pub extern "C" fn $mol(x: f64, y: f64) -> f64 {
            // SAFETY: as above.
            unsafe {
                *__errno_location() = 0;
                let r = $host(x, y);
                let e = *__errno_location();
                if e != 0 {
                    mol_syscall::errcell::set(mol_syscall::errno::from_linux(e));
                }
                r
            }
        }
    };
}

wrap1!(mol_sin, sin);
wrap1!(mol_cos, cos);
wrap1!(mol_exp, exp);
wrap1!(mol_log, log);
wrap2!(mol_pow, pow);
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
    fn domain_error_sets_guest_errno() {
        mol_syscall::errcell::set(0);
        let r = mol_sqrt(-1.0);
        assert!(r.is_nan());
        // glibc sets EDOM(33); Darwin EDOM is likewise 33.
        assert_eq!(mol_syscall::errcell::get(), 33);
        mol_syscall::errcell::set(0);
    }
}
