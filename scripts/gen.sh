#!/usr/bin/env bash
# Regenerates every generated file. CI fails if any of them is out of date.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run -q -p laneway-server -- config docs
cargo run -q -p laneway-server -- openapi > web/openapi.json
(cd web && pnpm -s gen:api)
cargo sqlx prepare --workspace -- --all-targets
