# Contributing

## Set up

Requirements: stable Rust (pinned in `rust-toolchain.toml`; `Option::is_none_or` needs ≥ 1.82), a C compiler
(Tree-sitter grammars are compiled by `cc`), `git`, and Node ≥ 22.6 for the frontend and scripts.

```bash
git clone <repo> && cd changegraph
scripts/check-all.sh         # or: make check   (SKIP_FRONTEND=1 to skip the Node checks)
scripts/demo.sh              # see it work on the bundled fixture
```

Commit `Cargo.lock` and `frontend/package-lock.json` once they exist, then switch CI to `--locked` / `npm ci`.

## What `check-all` runs (and what CI runs)

| Check | Command |
|-------|---------|
| Formatting | `cargo fmt --all -- --check` |
| Lints, warnings as errors | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust tests | `cargo test --workspace` |
| Documentation links | `node scripts/check-doc-links.mjs` |
| Script tests | `node --test "scripts/*.test.mjs" "eval/historical/*.test.mjs"` |
| Frontend | `npm run typecheck && npm run lint && npm test && npm run build` in `frontend/` |

## The test suites

| Suite | Where | What it protects |
|-------|-------|------------------|
| Unit tests | `#[cfg(test)]` in each `crates/changegraph-core/src/*.rs` | Parsing, resolution, graph algorithms, impact ranking, test mapping, metrics |
| Integration tests (git) | `crates/changegraph-core/tests/analyze.rs` | Real repositories: merge-base vs working tree, renames, deletions, errors, determinism |
| Fixture tests | `tests/graph.rs`, `tests/impact.rs`, `tests/test_selection.rs` | Real graphs from `fixtures/` against **hand-labeled** `fixtures/expected/*.json` |
| Schema conformance | `tests/schema.rs` | Emitted reports validate against `docs/schema/report-<version>.schema.json` |
| Golden file | `tests/golden.rs` | The viewer's sample report equals real analyzer output |
| CLI end-to-end | `crates/changegraph-cli/tests/cli.rs` | Exit codes, flags, output formats, atomic writes |
| Accuracy regression | `crates/changegraph-eval/tests/eval.rs` | Labeled scenarios and generated ground truth must match exactly |
| Viewer logic | `frontend/src/lib/*.test.ts` | Validation, model, filters, layout, canvas description |

A change that alters output on purpose updates the golden file
(`UPDATE_GOLDEN=1 cargo test -p changegraph-core --test golden`) and the affected expected files in the
same commit, and says why.

## Rules that keep the tool honest

1. **Never present a heuristic as a fact.** New relationships need a `basis` (known vs heuristic) and an
   explanation in the report.
2. **Never drop information silently.** Unresolved imports, parse failures, excluded files and truncated
   traversals become data or warnings.
3. **Labels come from reading the code, not from the tool.** When adding a fixture or scenario, write the
   expected result by hand first, then run the tool and investigate any difference; do not paste output into
   the expectation. See `fixtures/expected/README.md`.
4. **No invented numbers** in docs, tests or samples. Measurements come from `changegraph-eval` output
   committed under `eval/results/`.
5. **Determinism**: sorted output, no hash-map iteration order in results, no wall-clock values except
   `execution.duration_ms`.
6. No LLM in the analysis engine; no databases, queues or services without a demonstrated need.

## Changing the report schema

1. Edit the Rust types in `report.rs` (and the module that owns the new data).
2. Bump `SCHEMA_VERSION`; add `docs/schema/report-<new>.schema.json` and delete the old file.
3. Update `docs/report-schema.md` (including the "changes from" line).
4. `cd frontend && npm run sync-schema`, bump `SUPPORTED_SCHEMA_VERSION`, update types and components.
5. `cargo test --workspace` (schema conformance and golden tests will tell you what drifted) and
   `npm test`.

## Adding a scenario for the evaluation harness

Add an entry to a file in `eval/scenarios/` (see `docs/evaluation.md`): fixture, changes, and **labels
written by hand**. Run `cargo test -p changegraph-eval`.

## Pull requests

Say what changed, which checks you ran, and what you could not run. Link the limitation or doc you updated.
