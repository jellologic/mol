// SPDX-License-Identifier: MIT OR Apache-2.0
//! Bind/opcode engine: chained fixups, legacy binds, exports trie,
//! closure policy (docs 03, 12 L1/L3).
//!
//! Contract: pure binding logic ported from dyld OSS semantics.
//! C shims own dyld entry, TLS asm, and commpage only (doc 07) —
//! no guest-mapped async, ever.
