#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# P0.2.V1 arch-matrix check (relibc check.sh pattern, doc 13).
# Verifies: native check + tests, plus cross checks for both Linux
# targets (`cargo check` needs target std only, no linker).
set -eu
cd "$(dirname "$0")"
for target in x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu; do
  rustup target list --installed 2>/dev/null | grep -q "$target" \
    || rustup target add "$target"
done
cargo check --locked --workspace --all-targets
cargo check --locked --workspace --target x86_64-unknown-linux-gnu
cargo check --locked --workspace --target aarch64-unknown-linux-gnu
# The prefix-boot integration test needs mount powers (userns id-mapping),
# which stock CI runners deny; it runs in the privileged lane instead
# (tools/docker-boot-test.sh, P0.3.V1).
echo "check.sh: skipping empty_prefix_boots_pid1 (see privileged lane)"
cargo test --locked --workspace -- --skip empty_prefix_boots_pid1
echo "check.sh: matrix green"
