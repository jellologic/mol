// SPDX-License-Identifier: MIT OR Apache-2.0
//! Host-wrap table (elf_calls-style) + wrapgen equivalent (doc 07).
//!
//! Contract: the sole FFI seam from guest code to host libraries.
//! All `unsafe` in this crate is reviewed against the wrapped host
//! ABI; wrapped calls never expose host errno/layout quirks raw.
