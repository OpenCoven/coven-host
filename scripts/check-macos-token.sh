#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "$(uname -s)" != Darwin ]]; then
    echo "Run this check on macOS with an APFS temporary directory." >&2
    exit 1
fi
if (( $# )); then
    python3 tests/macos_token_recovery.py "$1"
else
    cargo build --locked --example compat_client
    target_dir=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')
    python3 tests/macos_token_recovery.py "$target_dir/debug/examples/compat_client" --adapter
fi
