// SPDX-License-Identifier: MIT OR Apache-2.0
//! Metal API over a Vulkan core (doc 05).
//!
//! Contract: edge adapter, not compat core. Adopted graphics deps
//! (ash et al, doc 24 verdicts) stay behind a narrow seam and never
//! leak guest-visible quirks (doc 23). Family feature-parity matrix
//! is the tracking artifact (P5.1.1).
