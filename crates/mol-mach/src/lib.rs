// SPDX-License-Identifier: MIT OR Apache-2.0
//! Ports/rights/port-sets + Mach message transport (doc 04).
//!
//! Contract: msg-id → reply semantics differential-clean vs the
//! oracle (F2 T0). Reactor starts mio/std-threaded; tokio with
//! `rt+sync` only if spawned tasks genuinely win, never
//! `tokio/full` (doc 23).
