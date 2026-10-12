// SPDX-License-Identifier: MIT OR Apache-2.0
//! Targeted: legacy rebase/bind opcode walkers.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = mol_macho::opcodes::walk_rebase(bytes, 1 << 16);
    let _ = mol_macho::opcodes::walk_bind(bytes, 1 << 16);
});
