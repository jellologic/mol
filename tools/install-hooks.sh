#!/bin/sh
# SPDX-License-Identifier: MIT OR Apache-2.0
# Install the mol git hooks (repo-local config, not committed).
set -eu
root="$(git rev-parse --show-toplevel)"
cd "$root"
chmod +x .githooks/commit-msg .githooks/pre-commit tools/mol-track
git config core.hooksPath .githooks
echo "hooks installed: core.hooksPath=$(git config core.hooksPath)"
echo "policy: every commit needs a tracker ref, e.g. [P1.1] or"
echo "        a 'Tracker: P1.1.1' trailer. Escape: --no-verify."
