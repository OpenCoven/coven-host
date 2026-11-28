#!/usr/bin/env bash
# Run the repository checks. CI calls this script, so local and CI results match.
#
#   scripts/check.sh            same as --fast
#   scripts/check.sh --fast     rust, hook and kit
#   scripts/check.sh --full     adds compat (Linux only)
#   scripts/check.sh STEP...    run the named steps only: rust, hook, kit, compat
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cd "$script_dir/.."

usage() {
    sed -n '2,7s/^# \{0,1\}//p' "$script_dir/check.sh"
}

steps=()
for arg in "$@"; do
    case "$arg" in
        --fast) steps+=(rust hook kit) ;;
        --full) steps+=(rust hook kit compat) ;;
        rust | hook | kit | compat) steps+=("$arg") ;;
        -h | --help) usage; exit 0 ;;
        *) echo "Unknown argument: $arg" >&2; usage >&2; exit 2 ;;
    esac
done
[ ${#steps[@]} -gt 0 ] || steps=(rust hook kit)

passed=()
not_run=()
need_python() {
    local candidate
    for candidate in python3 python; do
        if "$candidate" -c 'import sys; sys.exit(sys.version_info < (3, 11))' 2>/dev/null; then
            python=$candidate
            return
        fi
    done
    echo "The $1 step needs Python 3.11 or later on PATH." >&2
    exit 1
}

run() {
    echo "+ $*"
    "$@"
}

for step in "${steps[@]}"; do
    case " ${passed[*]:-} ${not_run[*]:-} " in
        *" $step "*) continue ;;
    esac
    echo "== $step"
    case "$step" in
        rust)
            run cargo fmt --check
            run cargo clippy --locked --all-targets -- -D warnings
            run cargo test --locked --all-targets
            run cargo build --locked --example compat_client
            ;;
        hook)
            if ! command -v gitleaks >/dev/null 2>&1; then
                echo "The hook step needs Gitleaks 8.30.1 or a compatible v8 release. See AGENTS.md." >&2
                exit 1
            fi
            run ./tests/pre-commit.sh
            ;;
        kit)
            need_python kit
            run "$python" -m unittest discover -s tests -p test_offline_kit.py -v
            ;;
        compat)
            # The historical fixture and the Rust exchange path rely on Linux renameat2(RENAME_EXCHANGE).
            if [ "$(uname -s)" != Linux ]; then
                echo "NOT RUN: compat needs Linux with a filesystem that supports atomic exchange, such as ext4." >&2
                echo "Run it on Linux, or rely on the CI compat job." >&2
                not_run+=(compat)
                continue
            fi
            need_python compat
            run env COVEN_TEST_PYTHON="$python" ./tests/compat-linux.sh
            ;;
    esac
    passed+=("$step")
done
echo "== passed: ${passed[*]:-none}"
if [ ${#not_run[@]} -gt 0 ]; then
    echo "== not run: ${not_run[*]}"
fi
