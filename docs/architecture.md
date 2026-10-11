# Architecture

```
            ┌──────────────┐   AnalysisConfig    ┌────────────────────────────┐
 user ────▶ │ changegraph  │ ──────────────────▶ │ changegraph-core           │
            │ (CLI, clap)  │ ◀────────────────── │  repository  git_diff      │
            └──────────────┘   Report / Error    │  discovery   report        │
                  │                              │  error       config        │
                  ▼ JSON file                    └────────────────────────────┘
            ┌──────────────┐                                 │ spawns
            │ frontend     │ (Phase 6, reads JSON only)      ▼
            └──────────────┘                               `git` CLI
```

**Principles**

- The core crate has no dependency on the CLI or the frontend; the CLI needs no server.
- Only `git.rs` spawns processes. All git output parsing is in pure functions with unit tests.
- Output is deterministic: sorted collections, no `HashMap` iteration in output, no user/global
  ignore files consulted. The sole non-deterministic field is `execution.duration_ms`.
- Nothing is silently hidden: excluded changed files are listed with `excluded: true`; walk
  errors and dirty-tree conditions become typed warnings.
- The report is streamed to its destination (`serde_json::to_writer_pretty`) rather than built
  as a `String`.

**Dependency choices**

| Crate | Why |
|-------|-----|
| `clap` (derive) | CLI parsing, `--help`, usage errors |
| `serde`, `serde_json` | Versioned report schema |
| `thiserror` | Typed errors with stable exit codes (`anyhow` not needed: no ad-hoc error plumbing yet) |
| `ignore` | `.gitignore`-aware walking, same engine as ripgrep |
| `tracing` / `tracing-subscriber` | Diagnostics on stderr, never stdout |
| `tempfile` (dev) | Throwaway git repos in tests |

Planned (Phase 2+): `tree-sitter` + `tree-sitter-typescript`/`-javascript`, `petgraph` (or
custom CSR adjacency with a stored reverse index), `insta` for snapshots.

**Planned modules** (not present yet, deliberately): `parser`, `symbols`, `dependency_graph`,
`impact`, `test_mapping`.

**Why Rust:** deterministic, fast whole-repo parsing and graph traversal in a single static
binary with no runtime dependency. Complexity is justified in the parser/graph/impact core; it
is not justified elsewhere, which is why the frontend stays a thin JSON consumer.
