#!/usr/bin/env bash
# Exercise the runner from an unrelated directory using a disposable Python stub.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd -P)
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
cd "$scratch"

"$root/scripts/check.sh" --help >help.txt 2>errors.txt
grep -q -- '--full' help.txt
test ! -s errors.txt
status=0
"$root/scripts/check.sh" typo >help.txt 2>errors.txt || status=$?
test "$status" -eq 2
test ! -s help.txt
grep -q 'Unknown argument: typo' errors.txt

cat > python3 <<'STUB'
#!/usr/bin/env bash
case "$1" in
    -c) exit 0 ;;
    -m) pwd -P >> "$RUNNER_TEST_LOG" ;;
    *) exit 1 ;;
esac
STUB
chmod +x python3
export RUNNER_TEST_LOG="$PWD/calls"
PATH="$PWD:$PATH" "$root/scripts/check.sh" kit kit >result.txt
test "$(cat calls)" = "$root"
grep -qx '== passed: kit' result.txt
