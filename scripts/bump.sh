#!/usr/bin/env bash
# ./dev.sh bump <major|minor|patch>
# Bumps the shared version (Cargo workspace + web/package.json) and commits `bump: vX.Y.Z`.
# Only on main: pushing that commit starts the release workflow.
set -euo pipefail
cd "$(dirname "$0")/.."

part="${1:-}"
[[ "$part" =~ ^(major|minor|patch)$ ]] || { echo "usage: ./dev.sh bump <major|minor|patch>" >&2; exit 2; }
branch="$(git rev-parse --abbrev-ref HEAD)"
[[ "$branch" == "main" ]] || { echo "bumps only happen on main (you're on $branch)" >&2; exit 1; }
git diff --quiet && git diff --cached --quiet || { echo "working tree not clean" >&2; exit 1; }

current="$(sed -n '/^\[workspace.package\]/,/^\[/ s/^version = "\(.*\)"/\1/p' Cargo.toml)"
IFS=. read -r major minor patch <<<"$current"
case "$part" in
  major) major=$((major + 1)); minor=0; patch=0 ;;
  minor) minor=$((minor + 1)); patch=0 ;;
  patch) patch=$((patch + 1)) ;;
esac
next="$major.$minor.$patch"

sed -i "/^\[workspace.package\]/,/^\[/ s/^version = \".*\"/version = \"$next\"/" Cargo.toml
sed -i "0,/\"version\": \".*\"/ s//\"version\": \"$next\"/" web/package.json
cargo update --workspace -q
# The OpenAPI spec carries the version too (CI checks it's up to date).
SQLX_OFFLINE=true cargo run -q -p laneway-server -- openapi > web/openapi.json
git add Cargo.toml Cargo.lock web/package.json web/openapi.json
git commit -q -m "bump: v$next"
echo "bumped $current -> $next. push main to release: git push origin main"
