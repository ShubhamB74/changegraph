# Historical datasets

A dataset records, for real commit pairs, which test files **newly fail** after the change. It is
the only input from which `changegraph-eval historical` can measure missed failing tests.

## Format (version 1)

```json
{
  "version": 1,
  "repo": "../../some/clone",
  "description": "free text: what project, how pairs were chosen",
  "cases": [
    {
      "id": "short label",
      "base": "<commit sha>",
      "head": "<commit sha>",
      "failing_after_change": ["tests/a.test.ts"],
      "notes": "optional"
    }
  ]
}
```

`repo` is relative to the dataset file (or absolute) and must be a Git clone containing both commits.
`failing_after_change` lists repo-relative test files that fail at `head` and pass at `base`.
Extra fields are ignored.

## Collecting one

```bash
printf '%s\n' "<base-sha> <head-sha>" "<base2> <head2>" > pairs.txt
node eval/historical/collect-outcomes.mjs \
  --repo /path/to/clone --pairs pairs.txt --out eval/historical/dataset.json \
  --setup "npm ci" --cmd "npx jest --json --outputFile={out}" --runs 2
```

`--cmd` must write a Jest- or Vitest-style JSON report to `{out}` (a non-zero exit code is expected
when tests fail). Each commit is checked out in a throwaway worktree, so your clone is not touched.
With `--runs N` a test counts as newly failing only if it fails in **every** head run and in **no**
base run. Both are guards against flaky tests, not guarantees.

Choose pairs with a stated rule (for example "every merged PR touching `src/` in 2025 that CI
marked failed at least once") and write it in `description`; hand-picked pairs bias the result.

Run the evaluation:

```bash
./target/release/changegraph-eval historical eval/historical/dataset.json --json eval/results/<date>-<project>-historical.json
```

Tests for the collector's parsing logic: `node --test "eval/historical/*.test.mjs"`.
