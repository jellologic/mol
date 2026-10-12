// SPDX-License-Identifier: MIT OR Apache-2.0
//! BSD syscall number translation + Darwin-only shims (doc 04).
//!
//! Contract: wrappers, not traps, are the compat boundary (doc 11
//! F1). Same errno names, same wrapper-observable side effects.
//! Raw-trap behavior is diagnostic signal only, never the gate.
