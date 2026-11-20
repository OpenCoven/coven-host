#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
reference=5c1f7349d20a72bc78b187b320e27319b9d5a078
if ! git cat-file -e "$reference^{commit}"; then
    echo "Fetch the compatibility fixture with: git fetch origin $reference" >&2
    exit 1
fi

fixture=$(mktemp -d)
trap 'rm -rf -- "$fixture"' EXIT
git archive "$reference" coven_host | tar -x -C "$fixture"
export PYTHONPATH="$fixture"

cargo build --locked --example compat_client
target_dir=$(cargo metadata --locked --no-deps --format-version 1 | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')
native="$target_dir/debug/examples/compat_client"
unset COVEN_TEST_HOST COVEN_TEST_CLIENT COVEN_TEST_NATIVE

if [[ ${1:-} == --deletions && $# == 1 ]]; then
    export COVEN_TEST_HOST="$native" COVEN_TEST_CLIENT="$native" COVEN_TEST_NATIVE="$native"
    export PYTHONPATH="$fixture:$PWD/tests"
    python3 -m unittest -v \
        test_integration.Integration.test_offline_rename_after_peer_restart \
        test_integration.Integration.test_peer_rename_retains_displaced_local_file_after_restart \
        test_integration.Integration.test_delete_edit_conflict_resolves_either_side \
        test_integration.Integration.test_delete_retry_after_lost_ack_and_restart \
        test_integration.Integration.test_deleted_path_rejects_peer_rollback
    exit
fi
if (( $# )); then
    echo "Usage: $0 [--deletions]" >&2
    exit 2
fi

echo "Historical client against Rust peer"
COVEN_TEST_HOST="$native" python3 -m unittest discover -s tests -v
echo "Rust client against historical peer"
COVEN_TEST_CLIENT="$native" python3 -m unittest discover -s tests -v
echo "Rust client and peer, including stopped-installation upgrade"
COVEN_TEST_HOST="$native" COVEN_TEST_CLIENT="$native" COVEN_TEST_NATIVE="$native" python3 -m unittest discover -s tests -v
