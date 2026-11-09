#!/usr/bin/env bash
# Rust test coverage with cargo-llvm-cov. Prints a per-file summary, writes target/lcov.info and
# fails when line coverage drops below the gate. Needs DATABASE_URL like the tests, plus
# `rustup component add llvm-tools-preview` and cargo-llvm-cov.
set -euo pipefail
cd "$(dirname "$0")/.."

# Overall line coverage, rounded down from the last measurement. Raise it as tests are added;
# targets are in TODO.md.
min_lines=83

if command -v cargo-nextest >/dev/null; then
  cargo llvm-cov nextest --no-report --workspace --locked --no-tests=pass "$@"
else
  cargo llvm-cov --no-report --workspace --locked "$@"
fi
cargo llvm-cov report --lcov --output-path target/lcov.info
cargo llvm-cov report --summary-only --fail-under-lines "$min_lines"

# Per-crate gates for the code that has to be the most trustworthy (see TODO.md for the targets).
cargo llvm-cov report --json --summary-only --output-path target/coverage.json
python3 scripts/coverage-gates.py target/coverage.json core=90 auth=90
