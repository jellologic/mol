// SPDX-License-Identifier: MIT OR Apache-2.0
//! mol-loader binary: Mach-O loading entry point.
//!
//! P0.3: `boot` wraps `unshare(1)`; hidden `__inner-boot` mounts the
//! overlay and execs init as PID 1. Real loading lands in P1.
//! Hand-rolled argv parsing per doc 25 — no CLI crate.
use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use mol_loader::prefix::PrefixSpec;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, PartialEq, Eq)]
struct BootArgs {
    root: Option<PathBuf>,
    lowerdir: Option<PathBuf>,
    upperdir: Option<PathBuf>,
    workdir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    init: Vec<String>,
}

/// Pure argv dispatch (no I/O) so unit tests exercise the real logic.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Help,
    Version,
    Boot(BootArgs),
    InnerBoot(BootArgs),
    UsageError(String),
}

fn empty_boot_args() -> BootArgs {
    BootArgs {
        root: None,
        lowerdir: None,
        upperdir: None,
        workdir: None,
        cache_dir: None,
        init: Vec::new(),
    }
}

fn take_value(
    argv: &[String],
    i: &mut usize,
    slot: &mut Option<PathBuf>,
) -> Result<(), String> {
    *i += 1;
    let val = argv
        .get(*i)
        .ok_or_else(|| "missing flag value".to_string())?;
    *slot = Some(PathBuf::from(val));
    Ok(())
}

/// Parse `boot`/`__inner-boot` flags. `--` separates flags from init argv.
fn parse_boot(argv: &[String]) -> Result<BootArgs, String> {
    let mut args = empty_boot_args();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--root" => take_value(argv, &mut i, &mut args.root)?,
            "--lowerdir" => take_value(argv, &mut i, &mut args.lowerdir)?,
            "--upperdir" => take_value(argv, &mut i, &mut args.upperdir)?,
            "--workdir" => take_value(argv, &mut i, &mut args.workdir)?,
            "--cache-dir" => take_value(argv, &mut i, &mut args.cache_dir)?,
            "--" => {
                args.init = argv[i + 1..].to_vec();
                break;
            }
            other => return Err(format!("unexpected boot flag '{other}'")),
        }
        i += 1;
    }
    if args.init.is_empty() {
        return Err("boot: missing init command after '--'".to_string());
    }
    Ok(args)
}

fn dispatch(argv: &[String]) -> Action {
    let head = argv.first().map(String::as_str);
    match head {
        None | Some("-h") | Some("--help") => Action::Help,
        Some("-V") | Some("--version") => Action::Version,
        Some("boot") => match parse_boot(&argv[1..]) {
            Ok(a) => Action::Boot(a),
            Err(e) => Action::UsageError(e),
        },
        Some("__inner-boot") => match parse_boot(&argv[1..]) {
            Ok(a) => Action::InnerBoot(a),
            Err(e) => Action::UsageError(e),
        },
        Some(other) => Action::UsageError(format!("unexpected argument '{other}'")),
    }
}

fn help_text() -> &'static str {
    "mol-loader: Mach-O loading entry point.\n\
     \n\
     usage: mol-loader [OPTIONS] COMMAND\n\
     \n\
     options:\n\
     \x20 -h, --help     print this help\n\
     \x20 -V, --version  print the version\n\
     \n\
     commands:\n\
     \x20 boot --root <prefix> [--lowerdir L --upperdir U --workdir W\n\
     \x20      --cache-dir C] -- <init> [args...]\n\
     \x20     boot an empty prefix: new mount+PID namespaces, overlay\n\
     \x20     merged view at <prefix>/merged, init exec'd as PID 1 (upper/work need overlay-capable fs)\n"
}

fn resolve_spec(args: &BootArgs) -> Result<PrefixSpec, String> {
    let root = args
        .root
        .clone()
        .ok_or_else(|| "boot: --root <dir> is required".to_string())?;
    let mut spec = PrefixSpec::with_defaults(&root);
    if let Some(p) = &args.lowerdir {
        spec.lowerdir = p.clone();
    }
    if let Some(p) = &args.upperdir {
        spec.upperdir = p.clone();
    }
    if let Some(p) = &args.workdir {
        spec.workdir = p.clone();
    }
    if let Some(p) = &args.cache_dir {
        spec.cache_dir = p.clone();
    }
    spec.validate_shape().map_err(|e| format!("boot: {e}"))?;
    Ok(spec)
}

#[cfg(target_os = "linux")]
fn run_boot(spec: &PrefixSpec, prefix: &Path, init: &[String]) -> i32 {
    use std::process::Command;
    let exe = match env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("mol-loader: cannot locate own binary: {e}");
            return 1;
        }
    };
    let mut cmd = Command::new("unshare");
    cmd.args(spec.unshare_argv());
    cmd.arg("--");
    cmd.arg(exe);
    cmd.arg("__inner-boot");
    // Pass the *prefix* dir, not the resolved merged view: inner
    // re-resolves defaults from --root, so passing spec.root would
    // double-apply them (<prefix>/merged/merged).
    cmd.arg("--root").arg(prefix);
    cmd.arg("--lowerdir").arg(&spec.lowerdir);
    cmd.arg("--upperdir").arg(&spec.upperdir);
    cmd.arg("--workdir").arg(&spec.workdir);
    cmd.arg("--cache-dir").arg(&spec.cache_dir);
    cmd.arg("--");
    cmd.args(init);
    match cmd.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("mol-loader: failed to run unshare: {e}");
            1
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn run_boot(_spec: &PrefixSpec, _prefix: &Path, _init: &[String]) -> i32 {
    eprintln!("mol-loader: prefix boot is Linux-only");
    1
}

#[cfg(target_os = "linux")]
fn run_inner_boot(spec: &PrefixSpec, init: &[String]) -> i32 {
    use mol_loader::prefix::linux;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    if let Err(e) = linux::create_layout(spec) {
        eprintln!("mol-loader: cannot create prefix layout: {e}");
        return 1;
    }
    match linux::check_devices(spec) {
        Ok(true) => {}
        Ok(false) => {
            eprintln!("mol-loader: workdir must share a filesystem with upperdir");
            return 1;
        }
        Err(e) => {
            eprintln!("mol-loader: cannot stat prefix dirs: {e}");
            return 1;
        }
    }
    if let Err(e) = linux::mount_all(spec) {
        eprintln!("mol-loader: {e}");
        return 1;
    }
    let mut cmd = Command::new(&init[0]);
    cmd.args(&init[1..]);
    for (k, v) in spec.guest_env() {
        cmd.env(k, v);
    }
    // Replaces this process: init lands as PID 1. Returns only on error.
    let err = cmd.exec();
    eprintln!("mol-loader: cannot exec init '{}': {err}", init[0]);
    1
}

#[cfg(not(target_os = "linux"))]
fn run_inner_boot(_spec: &PrefixSpec, _init: &[String]) -> i32 {
    eprintln!("mol-loader: prefix boot is Linux-only");
    1
}

fn main() -> ExitCode {
    let argv: Vec<String> = env::args().skip(1).collect();
    match dispatch(&argv) {
        Action::Help => {
            print!("{}", help_text());
            ExitCode::SUCCESS
        }
        Action::Version => {
            println!("mol-loader {VERSION}");
            ExitCode::SUCCESS
        }
        Action::Boot(args) => match resolve_spec(&args) {
            // Safe: resolve_spec errors when root is missing.
            Ok(spec) => {
                let prefix = args.root.as_ref().unwrap();
                ExitCode::from(run_boot(&spec, prefix, &args.init) as u8)
            }
            Err(e) => {
                eprintln!("mol-loader: {e}");
                ExitCode::from(2)
            }
        },
        Action::InnerBoot(args) => match resolve_spec(&args) {
            Ok(spec) => ExitCode::from(run_inner_boot(&spec, &args.init) as u8),
            Err(e) => {
                eprintln!("mol-loader: {e}");
                ExitCode::from(2)
            }
        },
        Action::UsageError(e) => {
            eprintln!("mol-loader: {e}");
            eprintln!("mol-loader: loading is not yet implemented (see P1.4)");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_is_help() {
        assert_eq!(dispatch(&args(&[])), Action::Help);
    }

    #[test]
    fn help_and_version_flags() {
        assert_eq!(dispatch(&args(&["-h"])), Action::Help);
        assert_eq!(dispatch(&args(&["--help"])), Action::Help);
        assert_eq!(dispatch(&args(&["-V"])), Action::Version);
        assert_eq!(dispatch(&args(&["--version"])), Action::Version);
    }

    #[test]
    fn boot_parses_flags_and_init() {
        let action = dispatch(&args(&[
            "boot", "--root", "/p", "--cache-dir", "/c", "--", "/bin/sh", "-c", "echo hi",
        ]));
        match action {
            Action::Boot(a) => {
                assert_eq!(a.root, Some(PathBuf::from("/p")));
                assert_eq!(a.cache_dir, Some(PathBuf::from("/c")));
                assert_eq!(a.init, vec!["/bin/sh", "-c", "echo hi"]);
            }
            other => panic!("expected Boot, got {other:?}"),
        }
    }

    #[test]
    fn boot_requires_init_after_dashdash() {
        assert!(matches!(
            dispatch(&args(&["boot", "--root", "/p"])),
            Action::UsageError(_)
        ));
    }

    #[test]
    fn resolve_spec_needs_root() {
        let err = resolve_spec(&empty_boot_args()).unwrap_err();
        assert!(err.contains("--root"), "unexpected: {err}");
    }

    #[test]
    fn resolve_spec_applies_defaults() {
        let mut a = empty_boot_args();
        a.root = Some(PathBuf::from("/p"));
        a.init = vec!["/bin/true".to_string()];
        let spec = resolve_spec(&a).unwrap();
        assert_eq!(spec.lowerdir, PathBuf::from("/p/lower"));
        assert_eq!(spec.cache_dir, PathBuf::from("/p/dyld-cache"));
    }

    #[test]
    fn unexpected_top_arg_is_usage_error() {
        assert!(matches!(
            dispatch(&args(&["--bogus"])),
            Action::UsageError(_)
        ));
    }
}
