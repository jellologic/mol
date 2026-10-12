// SPDX-License-Identifier: MIT OR Apache-2.0
//! xtask: repo codegen and fixtures. Run via `cargo xtask <task>`.
//!
//! Skeleton (P0.2): no tasks yet. Codegen tasks (syscall tables,
//! MIG stubs, fixture builders) arrive with the phases that need them.
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = env::args().skip(1).collect();
    match argv.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => {
            println!("xtask: repo codegen and fixtures (no tasks defined yet)");
            println!("usage: cargo xtask [--help]");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("xtask: unknown task '{other}' (no tasks defined yet)");
            ExitCode::from(2)
        }
    }
}
