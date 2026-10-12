// SPDX-License-Identifier: MIT OR Apache-2.0
//! XPC dict/array wire protocol over mol-mach (doc 12 L5).
//!
//! Contract: clean-room from public headers only — libxpc was never
//! open-sourced. Request/reply incl invalid-connection-on-crash
//! semantics (P2.2.1).
