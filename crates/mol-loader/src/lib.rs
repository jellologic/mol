// SPDX-License-Identifier: MIT OR Apache-2.0
//! Segment map + slide + entry transfer (doc 12 L1).
//!
//! Contract: owns guest address-space layout up to the dyld handoff.
//! Load maps must zero-diff against `otool -l` / `dyld_info` on the
//! thin+fat corpus (P1.1.V1). No framework code lives here.

pub mod prefix;
#[cfg(target_os = "linux")]
pub mod run;
pub mod stack;
#[cfg(target_os = "linux")]
pub(crate) mod sys_linux;
