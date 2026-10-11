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

## Planned (Phases 2–4)

- Edge semantics: `A -> B` means A imports B. Impact = reverse reachability from changed nodes,
  cycle-safe BFS with a visited set, shortest-path predecessor map for explanations.
- Ranking: transparent, deterministic (distance, direct vs transitive, number of changed
  dependencies). Not a calibrated probability.
