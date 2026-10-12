// SPDX-License-Identifier: MIT OR Apache-2.0
//! P0.3.V1: empty prefix boots PID 1.
//!
//! Linux-only (compiled out elsewhere). Needs mount powers: run as
//! root or in `docker run --privileged`. GHA ubuntu runners rely on
//! the userns flags in [`mol_loader::prefix::PrefixSpec::unshare_argv`];
//! if a runner kernel blocks that path this test fails loudly by design.

#![cfg(target_os = "linux")]

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn loader_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_mol-loader"))
}

#[test]
fn empty_prefix_boots_pid1() {
    let tmp = std::env::temp_dir().join(format!("mol-prefix-test-{}", std::process::id()));
    let root = tmp.join("root");
    let lower = tmp.join("lower");
    // Seed lowerdir content: proves the overlay merged view is live.
    fs::create_dir_all(&lower).expect("create lowerdir");
    fs::write(lower.join("hello.txt"), "from-lower\n").expect("seed lowerdir");

    let probe = "echo pid=$$; echo cache=$DYLD_SHARED_CACHE_DIR; cat $MOL_PREFIX/hello.txt";
    let out = Command::new(loader_bin())
        .args([
            "boot",
            "--root",
            root.to_str().unwrap(),
            "--lowerdir",
            lower.to_str().unwrap(),
            "--",
            "/bin/sh",
            "-c",
            probe,
        ])
        .output()
        .expect("run mol-loader boot");
    assert!(
        out.status.success(),
        "boot failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("pid=1"),
        "init is not PID 1: {stdout}"
    );
    let expected_cache = root.join("dyld-cache");
    assert!(
        stdout.contains(&format!("cache={}", expected_cache.display())),
        "cache override missing: {stdout}"
    );
    assert!(
        stdout.contains("from-lower"),
        "overlay content missing: {stdout}"
    );
    // Best-effort cleanup (namespaces self-destruct with init).
    let _ = fs::remove_dir_all(&tmp);
}
