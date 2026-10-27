#!/usr/bin/env bash
# Short demonstration on the bundled fixture: build a scratch Git repo from fixtures/simple-ts-project,
# change one file, and run the real analyzer. Requires: cargo, git.
#   scripts/demo.sh [output.json]
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$PWD/demo-report.json}"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

cp -R "$root/fixtures/simple-ts-project/." "$scratch/"
git -C "$scratch" init -q
git -C "$scratch" config user.email demo@example.com
git -C "$scratch" config user.name Demo
git -C "$scratch" config commit.gpgsign false
git -C "$scratch" add -A
git -C "$scratch" commit -q -m "fixture"

echo "# Editing src/auth/session.ts (uncommitted)"
printf '\n// demo change\n' >> "$scratch/src/auth/session.ts"

echo
echo "# changegraph analyze (human-readable)"
(cd "$root" && cargo run -q -p changegraph-cli -- analyze "$scratch")

(cd "$root" && cargo run -q -p changegraph-cli -- analyze "$scratch" --format json --output "$out")
echo
echo "# JSON report written to: $out"
echo "# Open it in the viewer: (cd frontend && npm run dev), then choose this file."
