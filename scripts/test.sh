#!/usr/bin/env bash
# All tests. Extra args go to cargo nextest.
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v cargo-nextest >/dev/null; then
  cargo nextest run --workspace --no-tests=pass "$@"
else
  cargo test --workspace "$@"
fi
(cd web && pnpm test)
