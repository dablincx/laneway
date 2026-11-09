#!/usr/bin/env bash
# Runs an image against a real Postgres 18 and checks it becomes ready and serves the web UI.
# Usage: scripts/smoke-test.sh <image>
set -euo pipefail
image="$1"
id="laneway-smoke-$$"
port="${SMOKE_PORT:-13000}"
cleanup() {
  docker logs "$id-app" 2>&1 | tail -40 || true
  docker rm -f "$id-app" "$id-db" >/dev/null 2>&1 || true
  docker network rm "$id" >/dev/null 2>&1 || true
}
trap cleanup EXIT

docker network create "$id" >/dev/null
docker run -d --name "$id-db" --network "$id" \
  -e POSTGRES_USER=laneway -e POSTGRES_PASSWORD=smoke -e POSTGRES_DB=laneway postgres:18 >/dev/null
for _ in $(seq 1 30); do
  docker exec "$id-db" pg_isready -U laneway >/dev/null 2>&1 && break
  sleep 1
done

docker run -d --name "$id-app" --network "$id" -p "$port:3000" \
  -e LANEWAY_PUBLIC_URL="http://localhost:$port" \
  -e LANEWAY_SECRET_KEY=smoke-test-secret-key-not-for-production \
  -e LANEWAY_DATABASE_URL="postgres://laneway:smoke@$id-db:5432/laneway" \
  "$image" >/dev/null

for _ in $(seq 1 60); do
  if curl -fsS "http://localhost:$port/readyz" >/dev/null 2>&1; then
    docker exec "$id-app" laneway health
    curl -fsS "http://localhost:$port/" | grep -q '<div id="root">' || { echo "web UI not served" >&2; exit 1; }
    curl -fsS "http://localhost:$port/api/v1/instance" | grep -q '"needs_setup":true' || { echo "API not answering" >&2; exit 1; }
    echo "smoke test ok"
    exit 0
  fi
  sleep 1
done
echo "laneway never became ready" >&2
exit 1
