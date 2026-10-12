// SPDX-License-Identifier: MIT OR Apache-2.0
//! Full pipeline: parse, select, decode, load-map.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|bytes: &[u8]| {
    if let Ok(file) = mol_macho::parse(bytes) {
        for (cpu, sub) in [
            (mol_macho::header::CPU_TYPE_ARM64, 0),
            (mol_macho::header::CPU_TYPE_X86_64, 3),
        ] {
            if let Ok(img) = file.select(cpu, sub) {
                let _ = img.decoded();
                let _ = img.load_map(0);
            }
        }
    }
});
