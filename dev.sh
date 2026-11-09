#!/usr/bin/env bash
# Runs laneway dev tasks inside the dev container. The host only needs Docker + Compose.
#
#   ./dev.sh up              start the dev env (Postgres, Mailpit, ntfy, dev container)
#   ./dev.sh services        start only Postgres, Mailpit, ntfy
#   ./dev.sh down            stop it
#   ./dev.sh shell           shell inside the dev container
#   ./dev.sh server          run the laneway server, rebuilding on change
#   ./dev.sh web             run the Vite dev server for the web UI
#   ./dev.sh test            all tests (Rust + web)
#   ./dev.sh lint            all formatting + lint checks CI runs
#   ./dev.sh fix             auto-format Rust + web
#   ./dev.sh gen             regenerate generated files (env docs, .env.example, .sqlx, API client)
#   ./dev.sh migrate         apply DB migrations to the dev database
#   ./dev.sh bump <major|minor|patch>   version bump commit (main only)
#   ./dev.sh <any command>   e.g. ./dev.sh cargo add serde
#
# LANEWAY_DEV_NATIVE=1 runs commands directly on the host instead of in the dev container
# (needs the toolchains). Services still run in Docker.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"
export LANEWAY_DEV_UID="${LANEWAY_DEV_UID:-$(id -u)}"
export LANEWAY_DEV_GID="${LANEWAY_DEV_GID:-$(id -g)}"
export LANEWAY_DEV_BUILD_JOBS="${LANEWAY_DEV_BUILD_JOBS:-$(( ($(nproc 2>/dev/null || echo 4) + 1) / 2 ))}"
COMPOSE=(docker compose -f docker/compose.dev.yml)

# Commit messages are checked locally too, not just in CI.
git config core.hooksPath scripts/git-hooks 2>/dev/null || true

NATIVE="${LANEWAY_DEV_NATIVE:-0}"
SERVICES=(postgres mailpit ntfy)

if [[ "$NATIVE" == "1" ]]; then
  # Same settings the dev container gets, but pointing at the published service ports.
  export LANEWAY_PUBLIC_URL="${LANEWAY_PUBLIC_URL:-http://localhost:5173}"
  export LANEWAY_SECRET_KEY="${LANEWAY_SECRET_KEY:-dev-secret-key-not-for-production-use-0000}"
  export LANEWAY_DATABASE_URL="${LANEWAY_DATABASE_URL:-postgres://laneway:laneway@localhost:5432/laneway}"
  export LANEWAY_DATA_DIRECTORY="${LANEWAY_DATA_DIRECTORY:-$ROOT/target/dev-data}"
  export LANEWAY_LOG_LEVEL="${LANEWAY_LOG_LEVEL:-debug}"
  export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-$LANEWAY_DEV_BUILD_JOBS}"
fi
# sqlx (compile-time checked queries, #[sqlx::test]) reads DATABASE_URL.
export DATABASE_URL="${DATABASE_URL:-${LANEWAY_DATABASE_URL:-postgres://laneway:laneway@localhost:5432/laneway}}"

services() {
  "${COMPOSE[@]}" up -d --wait "${SERVICES[@]}"
}

run() {
  if [[ "$NATIVE" == "1" ]]; then
    "$@"
  else
    if [[ -z "$("${COMPOSE[@]}" ps -q dev 2>/dev/null)" ]]; then up >&2; fi
    local tty=()
    [[ -t 0 && -t 1 ]] && tty=(-it)
    "${COMPOSE[@]}" exec "${tty[@]}" dev "$@"
  fi
}

up() {
  if [[ "$NATIVE" == "1" ]]; then
    services
    (cd web && pnpm install --frozen-lockfile)
    return
  fi
  "${COMPOSE[@]}" up -d --build --wait
  # Named volumes start out owned by root.
  "${COMPOSE[@]}" exec dev sudo chown "$LANEWAY_DEV_UID:$LANEWAY_DEV_GID" /cache /workspace/web/node_modules
  "${COMPOSE[@]}" exec dev bash -c 'cd web && pnpm install --frozen-lockfile'
}

cmd="${1:-help}"
[[ $# -gt 0 ]] && shift
case "$cmd" in
  help | -h | --help) awk 'NR > 1 && !/^#/ { exit } NR > 1 { sub(/^# ?/, ""); print }' "$0" ;;
  up) up ;;
  services) services ;;
  down) "${COMPOSE[@]}" down ;;
  shell) run bash ;;
  server) run cargo watch -x 'run -p laneway-server' ;;
  web) run bash -c 'cd web && pnpm dev' ;;
  test) run bash scripts/test.sh "$@" ;;
  lint) run bash scripts/lint.sh ;;
  fix) run bash -c 'cargo fmt --all && cd web && pnpm format' ;;
  gen) run bash scripts/gen.sh ;;
  migrate) run bash -c 'cd crates/db && cargo sqlx migrate run' ;;
  bump) run bash scripts/bump.sh "$@" ;;
  *) run "$cmd" "$@" ;;
esac
