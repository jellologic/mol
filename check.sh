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
cargo test --locked --workspace
echo "check.sh: matrix green"
