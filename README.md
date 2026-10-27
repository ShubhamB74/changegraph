# ChangeGraph

Local-first, deterministic change-impact analysis for TypeScript/JavaScript repositories, written in Rust.

**What it does.** It compares your Git changes with a base revision, parses TypeScript/JavaScript with
Tree-sitter, builds a file-level import graph, and reports which files and tests a change can affect, *why*
(the exact import chain), and what the analysis could not see. It recommends tests with a per-test
explanation, separating confirmed relationships from filename guesses, and says when the selection is
incomplete. An interactive viewer (`frontend/`) explores the JSON report. It is a static-import analysis,
not a proof: a recommendation never guarantees that a test will catch a regression.

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

## Evaluation

```bash
cargo build --release -p changegraph-cli -p changegraph-eval
./target/release/changegraph-eval scenarios
./target/release/changegraph-eval synthetic --sizes 100,1000
./target/release/changegraph-eval bench --sizes 100,1000,5000
```

What each experiment does and does not show: [docs/evaluation.md](docs/evaluation.md).

## Development

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Requires stable Rust (pinned via `rust-toolchain.toml`; `Option::is_none_or` needs Rust ≥ 1.82), a C compiler
(Tree-sitter grammars are compiled by `cc`) and `git` on `PATH`.
The first build generates `Cargo.lock` — commit it.

## Documentation

Start at [docs/index.md](docs/index.md). Highlights: [supported syntax](docs/supported-syntax.md),
[limitations](docs/limitations.md), [architecture and why Rust](docs/architecture.md),
[evaluation method](docs/evaluation.md), [roadmap](docs/roadmap.md), [contributing](CONTRIBUTING.md).

Try it on the bundled fixture: `scripts/demo.sh` ([walkthrough](docs/demo.md)).

## Layout

| Path | Purpose |
|------|---------|
| `crates/changegraph-core` | Engine: repository validation, git diff, discovery, report |
| `crates/changegraph-cli` | `changegraph` binary (clap) |
| `crates/changegraph-eval` | Evaluation/benchmark harness (labeled scenarios, synthetic ground truth, bench, historical) |
| `eval/` | Scenario labels, historical-dataset tooling, recorded results |
| `fixtures/` | Fixture projects + hand-labeled expected results |
| `docs/` | Architecture, algorithms, limitations, report schema |
| `frontend/` | Next.js + React Flow viewer for the JSON report (see `frontend/README.md`) |
