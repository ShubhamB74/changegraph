# ChangeGraph

Local-first, deterministic change-impact analysis for TypeScript/JavaScript repositories, written in Rust.

> **Status: Phase 6 of 8.** The engine validates a Git repository, identifies changed files
> (merge-base or working-tree mode), parses TypeScript/JavaScript with Tree-sitter, resolves
> imports (relative, `index`, tsconfig `paths`/`baseUrl`) and emits a versioned JSON report
> containing the dependency graph, unresolved/external imports, cycles and parse coverage.
> It then computes change impact: direct and transitive dependents of the changed files with
> explainable import paths, a transparent ranking, deleted/renamed-file handling, and an
> explicit `uncertain` class. It recommends tests with a per-test explanation, separating known
> relationships from filename guesses, and says when the selection is incomplete. An interactive
> viewer (`frontend/`) explores the JSON report. The evaluation harness and final docs polish
> (Phases 7-8) are **not implemented yet**. No LLM is used anywhere in the analysis engine.

## Quick start

```bash
# uncommitted changes (staged + unstaged + untracked) vs HEAD
cargo run -p changegraph-cli -- analyze .

# committed changes on HEAD since its merge base with <base>
cargo run -p changegraph-cli -- analyze . --base main

# machine-readable report
cargo run -p changegraph-cli -- analyze . --base main --format json -o report.json
```

There is no hardcoded default branch. Without `--base` the tool reports uncommitted work;
with `--base` it reports committed work and *warns* if the working tree is dirty. The two
modes are never mixed (details: [docs/algorithms.md](docs/algorithms.md)).

Options: `--base <rev>`, `--target <rev>` (requires `--base`, default `HEAD`),
`--format text|json`, `--compact`, `--no-timing`, `--max-depth <N>`, `--coverage-map <file>`, `--fail-on none|warnings|incomplete-tests`, `-x/--exclude <pattern>` (repeatable), `--no-default-excludes`,
`-o/--output <file>`, `-v/-vv`.

Exit codes: `0` ok · `1` internal/I-O · `2` usage/config · `3` path or repo problem ·
`4` revision problem · `5` git failure · `6` analysis succeeded but a `--fail-on` condition was met.

CI example: `changegraph analyze . --base origin/main --format json --no-timing -o report.json --fail-on incomplete-tests`
(the report is written even when the exit code is 6). A recommended-test list is **not** a guarantee: if
`test_recommendations.completeness.complete` is `false`, run the full suite.

## Viewer

```bash
cd frontend && npm install && npm run dev      # opens a bundled sample; open your own report.json in the page
```

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Requires stable Rust (pinned via `rust-toolchain.toml`; `Option::is_none_or` needs Rust ≥ 1.82), a C compiler
(Tree-sitter grammars are compiled by `cc`) and `git` on `PATH`.
The first build generates `Cargo.lock` — commit it.

## Layout

| Path | Purpose |
|------|---------|
| `crates/changegraph-core` | Engine: repository validation, git diff, discovery, report |
| `crates/changegraph-cli` | `changegraph` binary (clap) |
| `fixtures/` | Fixture projects + hand-labeled expected results |
| `docs/` | Architecture, algorithms, limitations, report schema |
| `frontend/` | Next.js + React Flow viewer for the JSON report (see `frontend/README.md`) |

## Roadmap

| Phase | Scope | State |
|-------|-------|-------|
| 1 | Repo validation, git diff, discovery, JSON report, CLI | **written; not yet compiled/tested by the author** |
| 2 | Tree-sitter import/export extraction, resolution, graph | **written; not yet compiled/tested by the author** |
| 3 | Reverse-traversal impact analysis with explainable paths | **written; not yet compiled/tested by the author** |
| 4 | Test recommendation with known-vs-heuristic distinction | **written; not yet compiled/tested by the author** |
| 5 | Report/CLI hardening: schema conformance test, atomic output, `--fail-on`, `--compact`, `--no-timing` | **written; not yet compiled/tested by the author** |
| 6 | Next.js + React Flow visualization | **written; logic layer tested, UI components not yet built/run by the author** |
| 7 | Evaluation harness and benchmarks | todo |
| 8 | Docs polish, demo, language-support roadmap | todo |

See [docs/limitations.md](docs/limitations.md) for what the tool does *not* claim.
