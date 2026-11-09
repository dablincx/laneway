#!/usr/bin/env bash
# Every check CI runs that doesn't need extra services.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo run -q -p laneway-server -- config docs --check
(cd web && pnpm lint && pnpm typecheck)
