#!/usr/bin/env bash
# Starts an example compose file the way a self-hoster would and checks laneway becomes ready.
# Usage: scripts/compose-test.sh <examples/compose file> <image>
set -euo pipefail
file="$(realpath "$1")"
image="$2"
dir="$(mktemp -d)"
project="laneway-compose-test-$$"
cp "$file" "$dir/"
cp "$(dirname "$file")/.env.example" "$dir/.env"
# Fill in the required values like a user would.
sed -i \
  -e 's|^LANEWAY_PUBLIC_URL=.*|LANEWAY_PUBLIC_URL=http://localhost:3000|' \
  -e 's|^LANEWAY_SECRET_KEY=.*|LANEWAY_SECRET_KEY=compose-test-secret-key-not-for-production|' \
  "$dir/.env"
echo "POSTGRES_PASSWORD=compose-test-password" >> "$dir/.env"
compose=(docker compose -p "$project" -f "$dir/$(basename "$file")" --project-directory "$dir")
cleanup() {
  "${compose[@]}" logs --tail 40 || true
  "${compose[@]}" down -v >/dev/null 2>&1 || true
  rm -rf "$dir"
}
trap cleanup EXIT

LANEWAY_IMAGE="$image" "${compose[@]}" up -d --wait --wait-timeout 120
curl -fsS http://localhost:3000/readyz >/dev/null
curl -fsS http://localhost:3000/api/v1/instance | grep -q '"needs_setup":true'
echo "$(basename "$file") ok"
