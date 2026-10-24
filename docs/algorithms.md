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

## Planned (Phase 4)

- Edge semantics: `A -> B` means A imports B. Impact = reverse reachability from changed nodes,
  cycle-safe BFS with a visited set, shortest-path predecessor map for explanations.
- Ranking: transparent, deterministic (distance, direct vs transitive, number of changed
  dependencies). Not a calibrated probability.
