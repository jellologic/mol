// SPDX-License-Identifier: MIT OR Apache-2.0
//! Targeted: chained-fixups parse + imports + chain walk (chains
//! walked against the input itself as image bytes).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(cf) = mol_macho::chained::ChainedFixups::parse(bytes) {
        let _ = cf.imports();
        let segs = [mol_macho::chained::SegRange {
            vmaddr: 0,
            vmsize: bytes.len() as u64,
            fileoff: 0,
            filesize: bytes.len() as u64,
        }];
        let _ = cf.each_fixup(bytes, &segs, 1 << 16, &mut |_| Ok(()));
    }
});
