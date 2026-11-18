#!/bin/sh
# Exercise the real hook in a disposable repository without touching the caller's index.
set -eu

repo=$(git rev-parse --show-toplevel)
testdir=$(mktemp -d)
trap 'rm -rf -- "$testdir"' EXIT
cd "$testdir"
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR
git init -q
git config core.hooksPath .githooks
git config user.name "Hook test"
git config user.email "hook-test@example.invalid"
git config commit.gpgsign false
cp -R "$repo/.githooks" "$repo/.gitleaks.toml" .
git add .githooks .gitleaks.toml
git commit -qm "Clean initial index"

expect_blocked() {
    if git commit -qm "Synthetic fixture" >hook.log 2>&1; then
        echo "Expected the commit guard to reject $1" >&2
        exit 1
    fi
}

# Generate a fresh value to keep credential-shaped literals out of the fixture.
token=$(openssl rand -hex 32)
printf '%s\n' "$token" > note
git add note
printf 'clean\n' > note
expect_blocked "staged token with a clean working copy"
if grep -Fq "$token" hook.log; then
    echo "Hook printed the synthetic token" >&2
    exit 1
fi

git add note
printf '%s\n' "$token" > note
git commit -qm "Clean staged bytes with an unstaged token"
mkdir access
printf '\000fixture' > access/token
git add access/token
expect_blocked "binary token file"
git rm -q --cached -- access/token
printf 'safe change\n' > note
git add note
git commit -qm "Remediated index"
echo "Commit guard passed staged-content, redaction and private-path checks."
