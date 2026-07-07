#!/usr/bin/env bash
set -euo pipefail

floor="$(tr -d '[:space:]' < .coverage-ratchet)"

if ! cargo llvm-cov --version >/dev/null 2>&1; then
  echo "cargo-llvm-cov is required; install cargo-llvm-cov 0.8.7 or newer" >&2
  exit 127
fi

mkdir -p target/coverage

cargo llvm-cov --locked --workspace \
  --summary-only \
  --json \
  --output-path target/coverage/coverage.json \
  --fail-under-lines "${floor}"

cargo llvm-cov report --lcov --output-path target/coverage/lcov.info
cargo llvm-cov report --text --output-path target/coverage/summary.txt
rm -rf target/coverage/html
cargo llvm-cov report --html --output-dir target/coverage
