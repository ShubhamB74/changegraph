#!/usr/bin/env bash
# Runs every check CI runs. Usage: scripts/check-all.sh   (SKIP_FRONTEND=1 to skip the Node checks)
set -euo pipefail
cd "$(dirname "$0")/.."

step() { printf '\n==> %s\n' "$*"; }

step "cargo fmt";    cargo fmt --all -- --check
step "cargo clippy"; cargo clippy --workspace --all-targets -- -D warnings
step "cargo test";   cargo test --workspace

step "documentation links"
node scripts/check-doc-links.mjs
node --test "scripts/*.test.mjs"
node --test "eval/historical/*.test.mjs"

if [ "${SKIP_FRONTEND:-0}" != "1" ]; then
  step "frontend"
  (cd frontend && npm install --no-audit --no-fund && npm run typecheck && npm run lint && npm test && npm run build)
fi

printf '\nAll checks passed.\n'
