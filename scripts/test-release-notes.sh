#!/usr/bin/env bash
# Regression test for scripts/release-notes.sh: a release with only internal commits must still
# change CHANGELOG.md (else the release workflow's commit fails) and get non-empty notes.
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git clone -q "$root" "$tmp/repo"
cd "$tmp/repo"
git config user.name "release test"
git config user.email "release-test@example.invalid"
# A release whose only commit is internal: start from the latest release, so app changes that
# are still unreleased on the current branch don't end up in it.
if latest="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null)"; then
  git checkout -q "$latest"
fi
git commit -q --allow-empty -m "fix(ci): something internal"
bash scripts/release-notes.sh 9.9.9 "$tmp/notes.md"

git diff --quiet CHANGELOG.md && { echo "CHANGELOG.md didn't change" >&2; exit 1; }
grep -q "^## 9\.9\.9 " CHANGELOG.md || { echo "no 9.9.9 section" >&2; exit 1; }
first="$(grep -m1 '^## ' CHANGELOG.md)"
[[ "$first" == "## 9.9.9 "* ]] || { echo "9.9.9 isn't the newest section: $first" >&2; exit 1; }
[[ -s "$tmp/notes.md" ]] || { echo "release notes are empty" >&2; exit 1; }
grep -q "internal" "$tmp/notes.md" || { echo "release notes don't explain the empty release" >&2; exit 1; }
echo "release notes test ok"
