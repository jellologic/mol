// SPDX-License-Identifier: MIT OR Apache-2.0
//! Print the canonical load map for a Mach-O file (golden helper).
//! usage: dump <file> [arm64|x86_64] [slide]
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.len() > 3 {
        eprintln!("usage: dump <file> [arm64|x86_64] [slide]");
        return ExitCode::from(2);
    }
    let bytes = match std::fs::read(&args[0]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("dump: cannot read {}: {e}", args[0]);
            return ExitCode::from(1);
        }
    };
    let (cputype, cpusubtype) = match args.get(1).map(String::as_str) {
        None | Some("arm64") => (mol_macho::header::CPU_TYPE_ARM64, 0),
        Some("x86_64") => (mol_macho::header::CPU_TYPE_X86_64, 3),
        Some(other) => {
            eprintln!("dump: unknown arch '{other}'");
            return ExitCode::from(2);
        }
    };
    let slide = args
        .get(2)
        .map(|s| {
            s.strip_prefix("0x")
                .map(|h| u64::from_str_radix(h, 16))
                .unwrap_or_else(|| s.parse())
                .unwrap_or(0)
        })
        .unwrap_or(0);
    // Thin files select by their own cputype; the arch arg only
    // matters for fat files. Try the requested arch first, then fall
    // back to whichever thin image this is.
    let file = match mol_macho::parse(&bytes) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("dump: parse failed: {e}");
            return ExitCode::from(1);
        }
    };
    let img = match file.select(cputype, cpusubtype) {
        Ok(img) => img,
        Err(_) => match &file {
            mol_macho::MachFile::Thin(img) => *img,
            mol_macho::MachFile::Fat(_) => {
                eprintln!("dump: no slice for requested arch");
                return ExitCode::from(1);
            }
        },
    };
    match img.load_map(slide) {
        Ok(map) => {
            print!("{}", map.canonical());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("dump: load map failed: {e}");
            ExitCode::from(1)
        }
    }
}
