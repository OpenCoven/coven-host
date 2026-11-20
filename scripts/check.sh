#!/usr/bin/env bash
# Run the repository checks. CI calls this script, so local and CI results match.
#
#   scripts/check.sh            same as --fast
#   scripts/check.sh --fast     rust, hook and kit
#   scripts/check.sh --full     adds compat (Linux only)
#   scripts/check.sh STEP...    run the named steps only: rust, hook, kit, compat
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

usage() {
    sed -n '2,7s/^# \{0,1\}//p' "$0" >&2
    exit 2
}

steps=()
for arg in "$@"; do
    case "$arg" in
        --fast) steps+=(rust hook kit) ;;
        --full) steps+=(rust hook kit compat) ;;
        rust | hook | kit | compat) steps+=("$arg") ;;
        -h | --help) usage ;;
        *) echo "Unknown argument: $arg" >&2; usage ;;
    esac
done
[ ${#steps[@]} -gt 0 ] || steps=(rust hook kit)

passed=()
not_run=()
python=$(command -v python3 || command -v python || true)

need_python() {
    if [ -z "$python" ]; then
        echo "The $1 step needs Python 3.11 or later on PATH." >&2
        exit 1
    fi
    "$python" -c 'import sys; sys.exit(sys.version_info < (3, 11))' || {
        echo "The $1 step needs Python 3.11 or later; found $("$python" --version 2>&1)." >&2
        exit 1
    }
}

run() {
    echo "+ $*"
    "$@"
}

for step in "${steps[@]}"; do
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
            run ./tests/compat-linux.sh
            ;;
    esac
    passed+=("$step")
done
echo "== passed: ${passed[*]:-none}"
if [ ${#not_run[@]} -gt 0 ]; then
    echo "== not run: ${not_run[*]}"
fi
