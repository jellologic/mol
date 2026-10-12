#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# P1.1.V2 AFL++ lane: bounded coverage-guided run over the mol-macho
# stdin harness. Ubuntu-focused (CI `fuzz` job); needs nightly-free
# stable Rust plus AFL++ and cargo-afl (installed on demand).
# Fails when the run reports crashes or hangs.
set -eu
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
DUR="${MOL_FUZZ_SECS:-120}"
OUT="${MOL_FUZZ_OUT:-/tmp/mol-fuzz}"
SEEDS="$OUT/seeds"

if [ "$(uname -s)" != "Linux" ]; then
  echo "fuzz-afl: Linux-only (run in the mol-dev container locally)" >&2
  exit 2
fi
if [ "$(id -u)" -eq 0 ]; then
  SUDO=""
else
  SUDO="sudo"
fi
if ! command -v afl-fuzz >/dev/null 2>&1; then
  echo "fuzz-afl: installing afl++"
  $SUDO apt-get update -qq
  $SUDO apt-get install -y -qq afl++ clang llvm
fi
if ! command -v cargo-afl >/dev/null 2>&1; then
  echo "fuzz-afl: installing cargo-afl"
  cargo install --locked cargo-afl --version 0.18.2
fi
if [ ! -x "$HOME/.cargo/bin/cargo-afl" ] && [ ! -x "$(command -v cargo-afl)" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

cd "$ROOT"
cargo afl build --locked -p mol-macho --example fuzz_stdin
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
HARNESS="$TARGET_DIR/debug/examples/fuzz_stdin"
if [ ! -x "$HARNESS" ]; then
  echo "fuzz-afl: harness not built at $HARNESS" >&2
  exit 1
fi

# Seeds: every corpus binary under each harness target byte.
rm -rf "$SEEDS"
mkdir -p "$SEEDS"
for target in 0 1 2 3 4 5 6 7; do
  for bin in corpus/macho/hello-fat corpus/macho/rich-arm64 corpus/macho/librich-fat.dylib corpus/macho/hello-x86_64; do
    name="$(basename "$bin")"
    { printf "\\$(printf '%03o' "$target")"; cat "$bin"; } > "$SEEDS/t${target}-${name}"
  done
done

echo "fuzz-afl: running ${DUR}s, out=$OUT"
export AFL_SKIP_CPUFREQ=1 AFL_NO_UI=1
# afl-fuzz refuses to run unless core dumps go to a plain file.
if [ -w /proc/sys/kernel/core_pattern ]; then
  echo core > /proc/sys/kernel/core_pattern
elif [ -n "$SUDO" ]; then
  echo core | $SUDO tee /proc/sys/kernel/core_pattern >/dev/null
else
  echo "fuzz-afl: core_pattern not writable; crashes may be missed" >&2
fi
rm -rf "$OUT/default"
cargo afl fuzz -V "$DUR" -m 512 -i "$SEEDS" -o "$OUT" -- "$HARNESS"

CRASHES="$(ls "$OUT/default/crashes" 2>/dev/null | grep -v README || true)"
HANGS="$(ls "$OUT/default/hangs" 2>/dev/null | grep -v README || true)"
if [ -n "$CRASHES" ] || [ -n "$HANGS" ]; then
  echo "fuzz-afl: FINDINGS crashes=[$CRASHES] hangs=[$HANGS]" >&2
  exit 1
fi
echo "fuzz-afl: clean (${DUR}s, no crashes/hangs)"
