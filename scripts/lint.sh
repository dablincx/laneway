#!/usr/bin/env bash
# Every check CI runs that doesn't need extra services.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -q -p laneway-server -- config docs --check
cargo sqlx prepare --workspace --check -- --all-targets
cargo run -q -p laneway-server -- openapi | diff -q - web/openapi.json >/dev/null \
  || { echo "web/openapi.json is out of date, run ./dev.sh gen" >&2; exit 1; }
cargo run -q -p laneway-server -- i18n check
(cd web && pnpm lint && pnpm typecheck)
