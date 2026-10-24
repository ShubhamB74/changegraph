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
| `tree-sitter` 0.24, `tree-sitter-typescript` / `-javascript` 0.23 | Syntax trees. tree-sitter is pinned to the 0.24 line (Node API changed in 0.25); grammar constants are used in exactly one function (`Language::grammar`). Requires a C compiler at build time |

Planned: `insta` for snapshots.

**Data flow (Phase 2):** `discovery` → files; `tsconfig::load` → alias config; `build_graph`
parses each file (`parser`), resolves each import (`resolver`), and assembles a
`DependencyGraph` (forward + reverse adjacency, cycle detection, reverse BFS), which is
converted into the report's `graph` section.

**Graph representation:** custom sorted adjacency lists rather than `petgraph`. The graph is
file-level, read-only after construction, and needs only reverse BFS and SCC — a few dozen lines
that keep the reverse index explicit and avoid an extra dependency. Revisit if Phase 3+ needs
weighted or symbol-level algorithms.

**Impact (Phase 3):** `impact::analyze_impact(&graph, &changed_files, tsconfig, max_depth)` borrows the
graph (before it is converted into the report) and returns an `ImpactReport` plus warnings. It uses
only the graph's public adjacency (`dependents`, `edge_between`, `unresolved_imports`).

**Planned modules** (not present yet, deliberately): `test_mapping`. A separate
`symbols` module was folded into `parser` (local export names) until symbol-level analysis is needed.

**Why Rust:** deterministic, fast whole-repo parsing and graph traversal in a single static
binary with no runtime dependency. Complexity is justified in the parser/graph/impact core; it
is not justified elsewhere, which is why the frontend stays a thin JSON consumer.
