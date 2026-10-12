// SPDX-License-Identifier: MIT OR Apache-2.0
//! relibc-style libSystem reimplementation (docs 07, 12 L2).
//!
//! Contract: init order is law — kernel → platform → pthread →
//! libc → malloc → dyld → dispatch → xpc, verified by traced
//! launch (P1.2.1/P2.2.V4). Guest-visible quirks are handrolled
//! behind trait seams (doc 23).
//!
//! This crate owns the guest-visible surface: the init-order
//! machine ([`init`]), the `libSystem.B.dylib` shim image
//! ([`shim`]), and the libm wrappers ([`math`], Linux-only).
//! Handler implementations live in `mol-syscall`.

pub mod init;
#[cfg(target_os = "linux")]
pub mod math;
pub mod shim;
pub mod ulp;

use shim::{Export, ExportKind};

/// Math export names (portable; implementations are Linux-only).
pub const MATH_EXPORTS: &[&str] = &[
    "_sin", "_cos", "_exp", "_log", "_pow", "_sqrt", "_floor", "_ceil", "_fmod", "_fabs",
];

/// The full shim registry in slot order: syscall exports first,
/// then math. Slot indexes are ABI (golden-tested).
pub fn exports() -> Vec<Export> {
    let mut out: Vec<Export> = mol_syscall::EXPORT_NAMES
        .iter()
        .map(|name| Export {
            name,
            kind: if *name == "___stack_chk_guard" {
                ExportKind::Data
            } else {
                ExportKind::Func
            },
        })
        .collect();
    out.extend(MATH_EXPORTS.iter().map(|name| Export {
        name,
        kind: ExportKind::Func,
    }));
    out
}

/// Handler address (or data value, for `___stack_chk_guard`) for a
/// shim export. The loader calls `mol_syscall::chk::init_guard`
/// before filling slots so the guard value is live.
#[cfg(target_os = "linux")]
pub fn resolve(name: &str) -> Option<u64> {
    if let Some(a) = mol_syscall::export_addr(name) {
        return Some(a);
    }
    math::export_addr(name)
}
