// SPDX-License-Identifier: MIT OR Apache-2.0
//! Fuzz harness: stdin bytes into every `mol-macho` parse entry point
//! (P1.1.V2). First byte selects the target; the rest is the input.
//! Exits 0 on any parse outcome — only panics, hangs, and sanitizer
//! traps are findings.
//!
//! AFL++: `cargo afl build -p mol-macho --example fuzz_stdin`, then
//! `afl-fuzz -i seeds -o out -- ./target/debug/examples/fuzz_stdin`
//! (see `tools/fuzz-afl.sh`). libFuzzer: `cargo fuzz run` targets in
//! `fuzz/fuzz_targets/` call the same entry points per target.

use std::io::Read;

fn main() {
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap_or(0);
    if input.is_empty() {
        return;
    }
    let (target, bytes) = (input[0] % 8, &input[1..]);
    match target {
        // Full pipeline: parse, select both arches, decode, load-map.
        0 => {
            if let Ok(file) = mol_macho::parse(bytes) {
                for (cpu, sub) in [
                    (mol_macho::header::CPU_TYPE_ARM64, 0),
                    (mol_macho::header::CPU_TYPE_X86_64, 3),
                ] {
                    if let Ok(img) = file.select(cpu, sub) {
                        let _ = img.decoded();
                        let _ = img.load_map(0);
                        let _ = img.load_map(0x1000);
                    }
                }
            }
        }
        // Headers: thin and fat.
        1 => {
            let _ = mol_macho::header::parse_header(bytes);
            let _ = mol_macho::header::parse_fat(bytes);
        }
        // Load-command table walk + full decode.
        2 => {
            if let Ok(h) = mol_macho::header::parse_header(bytes) {
                if let Ok(cmds) =
                    mol_macho::load::Commands::new(bytes, 0, h.ncmds, h.sizeofcmds)
                {
                    for raw in cmds {
                        match raw {
                            Ok(r) => {
                                let _ = mol_macho::load::decode(&r);
                            }
                            Err(_) => break,
                        }
                    }
                }
            }
        }
        // Legacy opcode streams.
        3 => {
            let _ = mol_macho::opcodes::walk_rebase(bytes, 1 << 16);
            let _ = mol_macho::opcodes::walk_bind(bytes, 1 << 16);
        }
        // Exports trie.
        4 => {
            let _ = mol_macho::trie::walk(bytes);
        }
        // Code signature.
        5 => {
            let _ = mol_macho::codesign::parse(bytes);
        }
        // Chained fixups: parse as payload, walk chains against the
        // input itself as image bytes (hostile self-reference).
        6 => {
            if let Ok(cf) = mol_macho::chained::ChainedFixups::parse(bytes) {
                let _ = cf.imports();
                let segs = [mol_macho::chained::SegRange {
                    vmaddr: 0,
                    vmsize: bytes.len() as u64,
                    fileoff: 0,
                    filesize: bytes.len() as u64,
                }];
                let mut n = 0usize;
                let _ = cf.each_fixup(bytes, &segs, 1 << 16, &mut |_| {
                    n += 1;
                    Ok(())
                });
                let _ = n;
            }
        }
        // Load-map dump rendering (exercises Display paths).
        _ => {
            if let Ok(file) = mol_macho::parse(bytes) {
                if let Ok(img) = file.select(mol_macho::header::CPU_TYPE_ARM64, 0) {
                    if let Ok(map) = img.load_map(0) {
                        let _ = map.canonical();
                    }
                }
            }
        }
    }
}
