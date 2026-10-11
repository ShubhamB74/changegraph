# ChangeGraph

Local-first, deterministic change-impact analysis for TypeScript/JavaScript repositories, written in Rust.

> **Status: Phase 1 of 8.** The engine validates a Git repository, identifies changed files
> (merge-base or working-tree mode), discovers source/test files, and emits a versioned JSON
> report. Dependency graph, impact analysis, test recommendations and the UI are **not
> implemented yet** — see the roadmap. No LLM is used anywhere in the analysis engine.

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
`--format text|json`, `-x/--exclude <pattern>` (repeatable), `--no-default-excludes`,
`-o/--output <file>`, `-v/-vv`.

Exit codes: `0` ok · `1` internal/I-O · `2` usage/config · `3` path or repo problem ·
`4` revision problem · `5` git failure.

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Requires stable Rust (pinned via `rust-toolchain.toml`) and `git` on `PATH`.
The first build generates `Cargo.lock` — commit it.

## Layout

| Path | Purpose |
|------|---------|
| `crates/changegraph-core` | Engine: repository validation, git diff, discovery, report |
| `crates/changegraph-cli` | `changegraph` binary (clap) |
| `fixtures/` | Fixture projects + hand-labeled expected results |
| `docs/` | Architecture, algorithms, limitations, report schema |
| `frontend/` | Placeholder; built in Phase 6 |

## Roadmap

| Phase | Scope | State |
|-------|-------|-------|
| 1 | Repo validation, git diff, discovery, JSON report, CLI | **written, not yet compiled/tested by the author** |
| 2 | Tree-sitter import/export extraction, resolution, graph | todo |
| 3 | Reverse-traversal impact analysis with explainable paths | todo |
| 4 | Test recommendation with known-vs-heuristic distinction | todo |
| 5 | Report schema hardening, depth/target options | todo |
| 6 | Next.js + React Flow visualization | todo |
| 7 | Evaluation harness and benchmarks | todo |
| 8 | Docs polish, demo, language-support roadmap | todo |

See [docs/limitations.md](docs/limitations.md) for what the tool does *not* claim.
