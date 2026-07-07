#!/usr/bin/env bash
set -euo pipefail

npm ci --prefer-offline --no-audit --no-fund
if [ "${CI:-}" = "true" ] || [ "$(uname -s)" = "Linux" ]; then
  npx playwright install --with-deps chromium
else
  npx playwright install chromium
fi
npm run e2e
