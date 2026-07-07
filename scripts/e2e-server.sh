#!/usr/bin/env bash
set -euo pipefail

port="${GLASS_E2E_PORT:-19041}"
db="${GLASS_E2E_DB:-target/e2e/glass.db}"
seed_out="${GLASS_E2E_SEED_OUT:-target/e2e/seed.json}"

mkdir -p "$(dirname "$db")" "$(dirname "$seed_out")"
rm -f "$db" "$db-shm" "$db-wal"

cargo run --quiet -- publish \
  --db "$db" \
  --title "Rendered e2e seed" \
  --agent "e2e-agent" \
  --session-title "Rendered e2e lane" \
  --surfaces-json tests/e2e/fixtures/seed-surfaces.json \
  --json > "$seed_out"

exec cargo run --quiet -- serve --bind "127.0.0.1:${port}" --db "$db"
