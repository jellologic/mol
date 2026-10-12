// SPDX-License-Identifier: MIT OR Apache-2.0
//! Targeted: exports-trie walker.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    let _ = mol_macho::trie::walk(bytes);
});
