// SPDX-License-Identifier: MIT OR Apache-2.0
//! Byte-exact golden load-map tests (P1.1.V1). Runs everywhere,
//! including Linux CI where `otool` is absent: the goldens were
//! verified field-by-field against `otool -l` / `dyld_info` /
//! `codesign` by `tests/oracle.rs` (macOS) at generation time.
//!
//! To regenerate after an intentional format change:
//! `cargo run -p mol-macho --example dump -- corpus/macho/<f> <arch>`
//! per `FIXTURES`, then re-run `tests/oracle.rs` on macOS.
use std::path::{Path, PathBuf};

const FIXTURES: &[(&str, &str, u32, u32)] = &[
    ("hello-arm64", "arm64", mol_macho::header::CPU_TYPE_ARM64, 0),
    (
        "hello-x86_64",
        "x86_64",
        mol_macho::header::CPU_TYPE_X86_64,
        3,
    ),
    ("hello-fat", "arm64", mol_macho::header::CPU_TYPE_ARM64, 0),
    ("hello-fat", "x86_64", mol_macho::header::CPU_TYPE_X86_64, 3),
    ("rich-arm64", "arm64", mol_macho::header::CPU_TYPE_ARM64, 0),
    (
        "rich-x86_64",
        "x86_64",
        mol_macho::header::CPU_TYPE_X86_64,
        3,
    ),
    ("rich-fat", "arm64", mol_macho::header::CPU_TYPE_ARM64, 0),
    ("rich-fat", "x86_64", mol_macho::header::CPU_TYPE_X86_64, 3),
    (
        "librich-arm64.dylib",
        "arm64",
        mol_macho::header::CPU_TYPE_ARM64,
        0,
    ),
    (
        "librich-x86_64.dylib",
        "x86_64",
        mol_macho::header::CPU_TYPE_X86_64,
        3,
    ),
    (
        "librich-fat.dylib",
        "arm64",
        mol_macho::header::CPU_TYPE_ARM64,
        0,
    ),
    (
        "librich-fat.dylib",
        "x86_64",
        mol_macho::header::CPU_TYPE_X86_64,
        3,
    ),
];

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("corpus")
        .join("macho")
}

fn first_diff(a: &str, b: &str) -> String {
    for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
        if x != y {
            return format!("line {}:\n  ours:   {x}\n  golden: {y}", i + 1);
        }
    }
    format!(
        "line count differs: ours={} golden={}",
        a.lines().count(),
        b.lines().count()
    )
}

#[test]
fn golden_load_maps_zero_diff() {
    for (file, arch, cputype, cpusubtype) in FIXTURES {
        let dir = corpus();
        let bytes = std::fs::read(dir.join(file)).unwrap();
        let parsed = mol_macho::parse(&bytes).unwrap();
        let img = parsed.select(*cputype, *cpusubtype).unwrap();
        let map = img.load_map(0).unwrap();
        let ours = map.canonical();
        let golden = std::fs::read_to_string(dir.join(format!("{file}.{arch}.loadmap"))).unwrap();
        assert!(
            ours == golden,
            "{file}[{arch}] drifted:\n{}",
            first_diff(&ours, &golden)
        );
    }
}
