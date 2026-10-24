# Algorithms

## Changed-file detection (Phase 1)

Two comparison modes exist. They are selected explicitly and never mixed.

| Mode | Selected by | Git strategy | Includes |
|------|-------------|--------------|----------|
| `merge_base` | `--base <rev>` | `git merge-base base target` → `git diff <merge-base> <target>` (equivalent to `base...target`) | Committed changes on `target` since it diverged from `base` |
| `working_tree` | no `--base` | `git diff HEAD` + `git ls-files --others --exclude-standard` | Staged, unstaged and untracked (non-ignored) files |

Notes:

- `base...target` (merge-base) is used rather than `base..target` so that commits that landed on
  `base` after the branch point are **not** reported as changes on the feature branch.
- In `merge_base` mode uncommitted work is **not** included; if present, a
  `dirty_working_tree` warning is emitted rather than silently mixing modes.
- Revisions are resolved with `git rev-parse --verify --quiet <rev>^{commit}` to full SHAs,
  which are recorded in the report. Revisions beginning with `-` are rejected.
- With no base, the tool does **not** guess a default branch (`main`/`master`/...).
- Rename detection uses `git diff -M`. Untracked renames are not detectable and appear as an
  untracked file (and a deletion if the old file was tracked).
- Paths come from `-z` output, so spaces, quotes and Unicode are handled; non-UTF-8 path bytes
  are converted lossily.

## File classification

`ts, tsx, js, jsx, mjs, cjs, mts, cts` are analyzable. A file is a *test* if its name ends in
`.test.<ext>` / `.spec.<ext>` or it lives under a `__tests__` directory. This is a filename
heuristic and does not prove the file contains tests.

## Exclusions

Defaults: `node_modules, .git, dist, build, out, coverage, .next, .turbo`.
A pattern without `/` matches any path component; a pattern with `/` matches that relative path
and everything beneath it. Globs are not supported yet. `.gitignore` files inside the tree are
honoured; global/user ignore files and `.git/info/exclude` are deliberately not (reproducibility).

## Dependency graph (Phase 2)

**Edge semantics:** `A -> B` means module A imports module B. Nodes are analyzed files
(sorted by path; indices follow that order, so all traversals are deterministic). Both
`forward` and `reverse` adjacency lists are stored (sorted, deduplicated).

**Extraction** (Tree-sitter, top-level statements only): static `import`, `import type`,
side-effect `import "x"`, named/namespace re-exports (`export {..} from`, `export * [as ns] from`),
and local export names. Statement-level `type` modifiers are detected; per-specifier
`{ type A }` is not. Dynamic `import()`, `require()`, `import x = require()` and `export =`
are **not** extracted.

**Resolution** (see `resolver.rs` module docs for the authoritative order): `node:` → external;
absolute path → unresolved; relative → probe; tsconfig `paths` (longest prefix wins; a matched
alias that finds nothing is *unresolved*, not external); `baseUrl`; otherwise external.
Probe order: TS ESM mapping (`./a.js` → `a.ts`/`a.tsx`), exact, `+ts,tsx,d.ts,mts,cts,js,jsx,mjs,cjs`,
then `/index.*`. Only analyzed files can be targets, so an import into an excluded directory
is reported as unresolved rather than as a confirmed edge.

**Confirmed vs. uncertain:** only resolved internal imports are edges. External packages and
unresolved imports are listed separately and never counted as dependencies.

**Parse errors:** a file with syntax errors is `partial` — facts are taken from statements that
parsed, and a `parse_error` warning names the first error line. Unreadable files are `failed`
with a `read_error` warning. Nothing aborts the run.

**Cycles:** detected with an iterative Tarjan SCC (no recursion, safe on deep chains). Reverse
reachability (`reverse_reachable`) is a BFS with a visited array, so cycles cannot cause
infinite traversal and distances are shortest dependent chains. Phase 3 builds impact analysis
(paths, classifications, ranking) on this primitive.

## Change impact (Phase 3)

**Seeds.** Each changed, non-excluded source/test file that is in the graph becomes a seed.
Changed files that are not analyzed are listed in `impact.ignored_changed_files` with a reason
(`non_source_file`, `excluded`, `not_in_graph`). A changed `package.json`, lockfile or
`tsconfig*.json` additionally raises `unmodeled_config_change`.

**Traversal.** One cycle-safe reverse BFS per seed (visited array, sorted neighbour lists, so the
shortest path found is deterministic). Traversal continues *through* other changed files, so a
file that depends on two changed files, one via the other, counts both. Changed files themselves
are never listed as affected. Cost is O(seeds x (V+E)); `--max-depth N` bounds the walk and
`depth_limited` records whether it cut anything off.

**Classification.** `direct` = distance 1, `transitive` = distance >= 2 (distance is the shortest
chain to *any* changed file). `affected_by` lists every changed file that reaches the node, each
with its own shortest distance; `path_from_changed` and `hops` give the single shortest chain,
including the exact import statement (specifier and line) at each step.

**Ranking.** `distance` ascending, then `changed_dependency_count` descending, then `path`
ascending. This is an ordering heuristic: a closer dependent is more likely to be affected than a
distant one, and a file depending on several changed files has more ways to break. It is **not** a
probability or calibrated risk.

**Deleted and renamed files.** A removed path is not a node, and its importers appear as
unresolved imports. Unresolved `not_found` imports are re-resolved against the removed paths with
the normal resolver; matching importers become direct dependents of the removed file (and
traversal continues from them). For a rename, the new path is a normal changed file and the old
path is treated as removed.

**Uncertain.** A file the graph does not reach is `uncertain` if it has a non-asset unresolved
import (not already explained by a removed file) or a `partial`/`failed` parse: it may import
changed code in a way the graph cannot see. These are listed separately and never mixed into
`affected`. The list is deliberately conservative and can be long in repositories with many
unresolved imports; it is not computed when nothing changed.

**Graph/revision consistency.** The graph is built from the files on disk. In merge-base mode,
if the target is not the checked-out commit, `graph_target_mismatch` is raised: the changed-file
list describes the target, but the graph describes the working tree.

## Test recommendation (Phase 4)

**Methods.** `changed_test` (the test file changed); `direct_import` (the test imports a
changed/affected module — a confirmed edge); `dependency_reachability` (it reaches one through
confirmed imports); `coverage_map` (optional user-supplied per-test coverage); `filename_heuristic`
(fallback, below). The first four are *known*; a recommendation backed only by
`filename_heuristic` has `basis: heuristic` and is labelled as a guess everywhere it appears.

**Covered modules.** The impacted modules are the changed source files, the affected source files
and removed paths. For every changed or affected test, a forward BFS over imports finds which
impacted modules it reaches (distance and first hop recorded). Removed paths are not graph
nodes, so their distances come from the impact analysis.

**Heuristic fallback.** Only for impacted source modules with *no* known test. A test matches if
its base name equals the module's (ignoring `.test`/`.spec`/`.d`) and it is in the same directory,
a sibling `__tests__/`, or a mirrored directory (`src/x/a.ts` ↔ `tests/x/a.test.ts`, roots
`src|lib|app` ↔ `test|tests|__tests__|spec`). Heuristic matches are listed on the unmapped module
and never count as coverage.

**Ordering.** known before heuristic; then tier (changed test; graph-confirmed by distance to the
nearest *changed* module; coverage-map-only; heuristic-only); then more known covered modules; then
path.

**Completeness.** `completeness.complete` is false when any of: impacted modules lack a known test;
`--max-depth` truncated traversal; unaffected files have unresolved imports/partial parses;
changed source files were excluded or not in the graph; `package.json`/lockfile/tsconfig changed;
the graph does not match the target revision; or a real change produced no recommendation.
Changes confined to non-source files (docs, assets) do not trigger incompleteness by themselves.

**What this does not claim.** Reachability is file-level and over-approximate; a listed test may
not exercise the change, and an unlisted test may be affected through a dependency the graph cannot
see. Safe test *omission* is not claimed anywhere; measuring it is Phase 7.

## Output and exit behaviour (Phase 5)

- **Determinism.** Identical repository state, options and tool version give identical reports
  except `execution.duration_ms` (and `repository.root`, an absolute path). `--no-timing` sets the
  duration to `null`, making output byte-identical (tested). All collections are sorted or
  built in a deterministic order; hash maps are used only for lookups.
- **Memory.** The report is built once and serialized directly into the destination
  (`serde_json::to_writer[_pretty]` into a buffered writer); no intermediate `String` copy.
  `--compact` drops whitespace for large reports.
- **Files.** `--output` writes a temporary file beside the destination and renames it, so failures
  never leave a truncated or partial report, and nothing is created if the analysis fails.
- **CI gating.** `--fail-on warnings|incomplete-tests` exits 6 *after* the report is written.

## Planned (Phase 6+)

- Edge semantics: `A -> B` means A imports B. Impact = reverse reachability from changed nodes,
  cycle-safe BFS with a visited set, shortest-path predecessor map for explanations.
- Ranking: transparent, deterministic (distance, direct vs transitive, number of changed
  dependencies). Not a calibrated probability.
