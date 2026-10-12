#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# P0.3.V1 privileged lane: runs the prefix-boot test in a privileged
# Ubuntu container. Stock CI runners deny userns id-mapping
# (unshare: uid_map EPERM), so the boot test cannot run directly on
# the runner; the container provides mount powers. Used by CI and
# reproducible locally with docker installed.
set -eu
ROOT="$(git rev-parse --show-toplevel)"
IMG="${MOL_BOOT_IMG:-ubuntu:24.04}"
docker run --rm --privileged \
  -v "$ROOT:/mol:rw" \
  -e DEBIAN_FRONTEND=noninteractive \
  -e CARGO_TARGET_DIR=/tmp/mol-target \
  "$IMG" sh -ec '
    apt-get update -qq
    apt-get install -y -qq curl build-essential ca-certificates
    curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.98.1
    export PATH="$HOME/.cargo/bin:$PATH"
    mkdir -p /prefix-test
    mount -t tmpfs tmpfs /prefix-test
    cd /mol
    TMPDIR=/prefix-test cargo test --locked -p mol-loader --test prefix_boot
  '
