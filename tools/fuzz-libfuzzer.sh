#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# P1.1.V2 libFuzzer lane: bounded runs over targeted mol-macho parse
# functions via cargo-fuzz. Ubuntu-focused (CI `fuzz` job); installs
# nightly + cargo-fuzz on demand. Fails on any crash artifact.
set -eu
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
DUR="${MOL_FUZZ_SECS:-60}"
TARGETS="${MOL_FUZZ_TARGETS:-parse_file trie chained opcodes}"

if [ "$(uname -s)" != "Linux" ]; then
  echo "fuzz-libfuzzer: Linux-only (run in the mol-fuzz container locally)" >&2
  exit 2
fi
if ! rustup toolchain list 2>/dev/null | grep -q nightly; then
  echo "fuzz-libfuzzer: installing nightly"
  rustup toolchain install nightly -c rust-src --profile minimal
fi
if ! command -v cargo-fuzz >/dev/null 2>&1; then
  echo "fuzz-libfuzzer: installing cargo-fuzz"
  cargo install --locked cargo-fuzz --version 0.13.2
fi
export RUSTUP_TOOLCHAIN=nightly
export PATH="$HOME/.cargo/bin:$PATH"

cd "$ROOT"
fail=0
for target in $TARGETS; do
  echo "fuzz-libfuzzer: $target ${DUR}s"
  # Working corpus first (libFuzzer reads all dirs, writes new units
  # only to the first); committed seeds stay read-only.
  corpus="fuzz/corpus/$target"
  mkdir -p "$corpus"
  log="$(mktemp)"
  if ! cargo fuzz run "$target" "$corpus" fuzz/seeds -- -max_total_time="$DUR" -print_final_stats=1 >"$log" 2>&1; then
    echo "fuzz-libfuzzer: $target FAILED" >&2
    tail -20 "$log" >&2
    fail=1
  fi
  tail -2 "$log"
  rm -f "$log"
done
found="$(find fuzz/artifacts -type f 2>/dev/null | head -20)"
if [ -n "$found" ]; then
  echo "fuzz-libfuzzer: artifacts present:" >&2
  echo "$found" >&2
  fail=1
fi
if [ "$fail" -ne 0 ]; then
  exit 1
fi
echo "fuzz-libfuzzer: clean (${DUR}s x $TARGETS)"
