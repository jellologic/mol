// SPDX-License-Identifier: MIT OR Apache-2.0
//! Objective-C runtime shims + messenger ABI (docs 03, 12 L4).
//!
//! Contract: `+load` ordering is superclass-first, categories after
//! the parent class, verified on deep hierarchies (P1.3.1).
//! Messenger ABI incl nil-receiver, stret/fpret, ARC entry points
//! (P1.3.3). ABI-critical paths may drop to C.
