#!/usr/bin/env bash
# Starts an image and waits for it to become healthy. Usage: scripts/smoke-test.sh <image>
set -euo pipefail
image="$1"
name="laneway-smoke-$$"
trap 'docker logs "$name" 2>&1 | tail -50; docker rm -f "$name" >/dev/null 2>&1 || true' EXIT

docker run -d --name "$name" \
  -e LANEWAY_PUBLIC_URL=http://localhost:3000 \
  -e LANEWAY_SECRET_KEY=smoke-test-secret-key-0000000000000000 \
  -e LANEWAY_DATABASE_URL=postgres://laneway:laneway@localhost:5432/laneway \
  "$image" >/dev/null

for _ in $(seq 1 30); do
  if docker exec "$name" laneway health 2>/dev/null; then
    docker exec "$name" sh -c 'test -f /app/web/index.html' || { echo "web UI missing from image" >&2; exit 1; }
    echo "smoke test ok"
    exit 0
  fi
  sleep 1
done
echo "container never became healthy" >&2
exit 1
