# Recorded results

Commit the `--json` files written by `changegraph-eval` here, one per run, named
`YYYY-MM-DD-<machine>-<command>.json` (for `historical`, include the project name). Each file stores
the harness version, OS, architecture, CPU count and whether the harness was a release build, so a
number can always be traced to its conditions.

Rules:

- Only commit files produced by the harness. Never edit a result by hand.
- Do not average results from different machines.
- Quote a result with its interval or spread, and with the experiment's limits from
  `docs/evaluation.md`.
- Re-run after any change to extraction, resolution, impact or test selection, and say what changed.
