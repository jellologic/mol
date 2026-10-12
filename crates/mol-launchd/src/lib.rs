// SPDX-License-Identifier: MIT OR Apache-2.0
//! Plist spawn + bootstrap registry + socket activation (doc 12 L5).
//!
//! Contract: launchd-as-init owns the service lifecycle — on-demand
//! launch, restart on crash, SIGKILL when idle — proven by fault
//! injection (P2.2.2).
