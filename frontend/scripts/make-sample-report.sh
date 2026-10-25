#!/usr/bin/env bash
# Regenerates public/sample-report.json with the REAL analyzer, from fixtures/simple-ts-project.
# Until you run this, the committed sample is hand-assembled from the fixture's labeled expectations.
#
# The fixture lives inside this repository, so it is copied to a scratch Git repo first
# (the analyzer works on the enclosing repository). Requires: cargo, git.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="$here/../public/sample-report.json"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

cp -R "$root/fixtures/simple-ts-project/." "$scratch/"
git -C "$scratch" init -q
git -C "$scratch" config user.email sample@example.com
git -C "$scratch" config user.name Sample
git -C "$scratch" config commit.gpgsign false
git -C "$scratch" add -A
git -C "$scratch" commit -q -m "fixture"
printf '\n// sample change\n' >> "$scratch/src/auth/session.ts"

(cd "$root" && cargo run -q -p changegraph-cli -- analyze "$scratch" --format json --no-timing --output "$out")
echo "wrote $out"
echo "Now run: (cd frontend && npm test)  # confirms the real report is accepted by the viewer"
