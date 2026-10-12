// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared conformance helpers (P1.2.V1).

use std::ffi::CString;
use std::path::PathBuf;

/// Unique temp path per test (parallel-safe).
#[allow(dead_code)] // shared: each suite uses a subset
pub fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "mol-syscall-{}-{}-{name}",
        std::process::id(),
        // Nanos disambiguate reruns within one pid.
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[allow(dead_code)] // shared: each suite uses a subset
pub fn cstr(path: &std::path::Path) -> CString {
    CString::new(path.as_os_str().as_encoded_bytes()).unwrap()
}

/// Assert the guest errno cell holds `want` (number + Mac name).
#[allow(dead_code)] // shared: each suite uses a subset
pub fn assert_errno(want: i32) {
    let got = mol_syscall::errcell::get();
    assert_eq!(
        got,
        want,
        "errno {got} ({:?}) != {want} ({:?})",
        mol_syscall::errno::name(got),
        mol_syscall::errno::name(want)
    );
}
