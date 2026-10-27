#!/usr/bin/env bash
# Regenerates public/sample-report.json from REAL analyzer output.
# The single source of truth is the `golden` test, which builds the scenario and compares/rewrites the file.
# Requires: cargo, git.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
(cd "$root" && UPDATE_GOLDEN=1 cargo test -q -p changegraph-core --test golden sample_report_equals_real_analyzer_output)
echo "regenerated frontend/public/sample-report.json; review the diff, then run: (cd frontend && npm test)"
