#!/usr/bin/env bash
# Regression test for scripts/bump.sh: bumps a throwaway clone and checks that every file carrying
# the version agrees and the generated OpenAPI spec is up to date (what CI checks after a release).
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

git clone -q "$root" "$tmp/repo"
cd "$tmp/repo"
git config user.name "bump test"
git config user.email "bump-test@example.invalid"
git checkout -q -B main
# Reuse the build cache so this doesn't compile everything again.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$root/target}"

bash scripts/bump.sh patch >/dev/null
version="$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version = "\(.*\)"/\1/p' Cargo.toml)"
grep -q "\"version\": \"$version\"" web/package.json || { echo "web/package.json not bumped" >&2; exit 1; }
SQLX_OFFLINE=true cargo run -q -p laneway-server -- openapi | diff -q - web/openapi.json >/dev/null \
  || { echo "web/openapi.json is out of date after the bump" >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo "bump left uncommitted changes" >&2; git status --short >&2; exit 1; }
git log -1 --format=%s | grep -qx "bump: v$version" || { echo "unexpected bump commit message" >&2; exit 1; }
echo "bump test ok ($version)"
