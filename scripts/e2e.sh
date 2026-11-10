#!/usr/bin/env bash
# Playwright end-to-end tests (web/e2e) against a fresh laneway with an empty database.
#
#   scripts/e2e.sh [playwright args]                  cargo-built server + Vite dev server
#   scripts/e2e.sh --image <image> [playwright args]  the Docker image + a Postgres 18 container
#
# LANEWAY_E2E_PREVIEW=1 tests a production build of the web UI (vite build + vite preview)
# instead of the dev server, like the image does in CI. Timing bugs can show up only there.
#
# Local mode creates (and afterwards drops) its own database next to $LANEWAY_DATABASE_URL, named
# $LANEWAY_E2E_DATABASE (default laneway_e2e), and uses ports LANEWAY_E2E_PORT (server, default
# 3100) and LANEWAY_E2E_PORT + 1 (Vite). Image mode publishes the app on LANEWAY_E2E_PORT.
# Chromium must be installed for Playwright (`pnpm exec playwright install chromium`).
set -euo pipefail
cd "$(dirname "$0")/.."
root="$PWD"

image=""
if [[ "${1:-}" == "--image" ]]; then
  image="${2:?--image needs an image name}"
  shift 2
fi
port="${LANEWAY_E2E_PORT:-3100}"
tmp="$(mktemp -d)"
pids=()
db_url=""
docker_id=""
ok=0

cleanup() {
  for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  if [[ "$ok" != 1 ]]; then
    echo "--- laneway log (tail) ---" >&2
    if [[ -n "$docker_id" ]]; then docker logs "$docker_id-app" 2>&1 | tail -40 >&2 || true
    else tail -40 "$tmp/server.log" >&2 2>/dev/null || true; fi
  fi
  if [[ -n "$docker_id" ]]; then
    docker rm -f "$docker_id-app" "$docker_id-db" >/dev/null 2>&1 || true
    docker network rm "$docker_id" >/dev/null 2>&1 || true
  fi
  if [[ -n "$db_url" ]]; then DATABASE_URL="$db_url" sqlx database drop -y -f >/dev/null 2>&1 || true; fi
  rm -rf "$tmp"
}
trap cleanup EXIT

wait_for() {
  local url="$1"
  for _ in $(seq 1 120); do
    curl -fsS "$url" >/dev/null 2>&1 && return 0
    sleep 1
  done
  echo "$url never answered" >&2
  return 1
}

if [[ -n "$image" ]]; then
  docker_id="laneway-e2e-$$"
  docker network create "$docker_id" >/dev/null
  docker run -d --name "$docker_id-db" --network "$docker_id" \
    -e POSTGRES_USER=laneway -e POSTGRES_PASSWORD=e2e -e POSTGRES_DB=laneway postgres:18 >/dev/null
  for _ in $(seq 1 30); do
    docker exec "$docker_id-db" pg_isready -U laneway >/dev/null 2>&1 && break
    sleep 1
  done
  docker run -d --name "$docker_id-app" --network "$docker_id" -p "$port:3000" \
    -e LANEWAY_PUBLIC_URL="http://localhost:$port" \
    -e LANEWAY_SECRET_KEY=e2e-test-secret-key-not-for-production \
    -e LANEWAY_DATABASE_URL="postgres://laneway:e2e@$docker_id-db:5432/laneway" \
    "$image" >/dev/null
  wait_for "http://localhost:$port/readyz"
  export LANEWAY_E2E_BASE_URL="http://localhost:$port"
  export LANEWAY_E2E_LOG_COMMAND="docker logs $docker_id-app"
else
  web_port=$((port + 1))
  base="${LANEWAY_DATABASE_URL:-postgres://laneway:laneway@localhost:5432/laneway}"
  db_url="${base%/*}/${LANEWAY_E2E_DATABASE:-laneway_e2e}"
  DATABASE_URL="$db_url" sqlx database drop -y -f >/dev/null 2>&1 || true
  DATABASE_URL="$db_url" sqlx database create

  cargo build -q -p laneway-server
  # Only what the test instance needs; the dev settings from dev.sh stay out.
  env -u LANEWAY_INITIAL_ADMIN_EMAIL -u LANEWAY_INITIAL_ADMIN_PASSWORD -u LANEWAY_E2E_PREVIEW \
    LANEWAY_PUBLIC_URL="http://localhost:$web_port" \
    LANEWAY_SECRET_KEY="${LANEWAY_SECRET_KEY:-e2e-test-secret-key-not-for-production}" \
    LANEWAY_DATABASE_URL="$db_url" \
    LANEWAY_HTTP_PORT="$port" \
    LANEWAY_DATA_DIRECTORY="$tmp/data" \
    LANEWAY_LOG_LEVEL=info \
    NO_COLOR=1 \
    "${CARGO_TARGET_DIR:-$root/target}/debug/laneway" serve >"$tmp/server.log" 2>&1 &
  pids+=($!)
  wait_for "http://localhost:$port/readyz"

  vite_mode=()
  if [[ "${LANEWAY_E2E_PREVIEW:-}" == 1 ]]; then
    (cd web && node_modules/.bin/vite build >"$tmp/vite-build.log" 2>&1)
    vite_mode=(preview)
  fi
  # The vite binary itself, not `pnpm exec`: killing pnpm would leave vite running.
  (cd web && LANEWAY_DEV_SERVER_URL="http://localhost:$port" \
    exec node_modules/.bin/vite "${vite_mode[@]}" --port "$web_port" --strictPort >"$tmp/vite.log" 2>&1) &
  pids+=($!)
  wait_for "http://localhost:$web_port/"
  export LANEWAY_E2E_BASE_URL="http://localhost:$web_port"
  export LANEWAY_E2E_LOG_FILE="$tmp/server.log"
fi

(cd web && pnpm exec playwright test "$@")
ok=1
