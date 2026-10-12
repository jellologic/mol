// SPDX-License-Identifier: MIT OR Apache-2.0
//! Objective-C runtime: metadata parsing + `+load` ordering + messenger
//! ABI (docs 03, 12 L4).
//!
//! Contract: `+load` ordering is superclass-first, categories after
//! the parent class, exactly per `objc-loadmethod.mm`
//! (`call_load_methods` / detach-drain / outermost-only re-entrancy).
//! All metadata reads are bounds-checked (`Result`, never panic) so
//! hostile guests fail load instead of crashing the loader.

pub mod load;
pub mod meta;

use std::fmt;

/// ObjC metadata/ordering failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjcError {
    /// File offset outside the image bytes.
    Truncated { offset: usize, len: usize },
    /// Structurally invalid metadata (bad counts, sizes, cycles).
    Malformed { offset: u64, detail: &'static str },
}

impl fmt::Display for ObjcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjcError::Truncated { offset, len } => {
                write!(f, "objc metadata truncated at {offset:#x} (len {len:#x})")
            }
            ObjcError::Malformed { offset, detail } => {
                write!(f, "objc metadata malformed at {offset:#x}: {detail}")
            }
        }
    }
}

impl std::error::Error for ObjcError {}

pub type Result<T> = std::result::Result<T, ObjcError>;
