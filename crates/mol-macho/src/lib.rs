// SPDX-License-Identifier: MIT OR Apache-2.0
//! Mach-O/fat parsing boundary (doc 03).
//!
//! Contract: hostile input. Every public parse entry point must be
//! fuzzed (doc 13) and differentially checked against `llvm-objdump`
//! / `dyld_info`. No I/O, no syscalls, no/stdout policy here —
//! pure bytes-in, structures-out.
