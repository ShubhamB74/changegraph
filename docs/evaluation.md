# Evaluation

Building the graph is not the finish line; the question is whether the tool's answers are right and
useful. `changegraph-eval` measures that. **This repository contains no measured results yet** —
the harness was written without a Rust toolchain, and numbers are never written by hand. Run it,
and commit the JSON it produces (see `eval/results/README.md`).

```bash
cargo build --release -p changegraph-cli -p changegraph-eval
./target/release/changegraph-eval scenarios                                 # labeled scenarios
./target/release/changegraph-eval synthetic --sizes 100,1000,5000          # exact-truth repos
./target/release/changegraph-eval bench --sizes 100,1000,5000 --runs 5      # time and memory
./target/release/changegraph-eval historical eval/historical/dataset.json   # real commits (you build the dataset)
# each accepts --json FILE for a machine-readable result with provenance (OS, CPUs, build mode)
```

`cargo test --workspace` also runs the scenario and synthetic experiments as **regression tests**:
they assert exact agreement, so an accuracy regression fails CI.

## What each experiment can and cannot show

| Experiment | Ground truth | Answers | Does **not** answer |
|------------|--------------|---------|---------------------|
| `scenarios` | Hand-written labels on small fixture projects (what a person concludes by reading the code) | Does the implementation agree with a careful human reading, including edge cases: cycles, re-exports, type-only and side-effect imports, aliases, JS-extension imports, deleted/renamed files, parse errors, excluded directories? | Anything about large or unusual real code. Labels are written by the same people who wrote the tool. |
| `synthetic` | Exact, by construction: the generator decides every import and records it | Precision/recall of dependency extraction and of impact analysis, and whether shortest distances are right, at any size | Whether the tool generalises. The repositories are clean, acyclic, and use one import style. A perfect score proves correctness on well-formed input only. |
| `bench` | n/a | Wall time and peak memory of the real CLI binary versus repository size | Behaviour on real repositories (shape, file sizes, depth) or on other hardware |
| `historical` | Recorded outcomes of a project's real test suite at real commit pairs | Of the tests that **actually broke**, how many were recommended, and how much of the suite would have been skipped? | Regressions that no test caught; flaky failures are a threat (see below) |

Only `historical` can say anything about "missed failing tests" or the safety of skipping tests.
Until a dataset exists, **do not claim that omitting unrecommended tests is safe.**

## Metric definitions

All set comparisons use *predicted* P and *truth* T.

- **Precision** = |P ∩ T| / |P|. **Recall** = |P ∩ T| / |T|. **F1** = 2PR / (P + R).
  When the denominator is 0 the metric is **undefined and printed `n/a`**, never 0 or 100%
  (an empty prediction against an empty truth is *exact*, but its precision/recall are undefined).
- **Micro-average** over scenarios: sum tp/fp/fn first, then divide. A scenario with a large
  truth set weighs more than one with a small set.
- **Dependency extraction** is scored on the set of `(importer, imported)` internal edges; external
  packages on `(importer, package)`; unresolved imports on `(importer, specifier)`.
- **Impact** is scored on the set of affected files (the changed files are excluded from both
  sides); **distance exactness** counts correctly found files whose reported shortest distance
  equals the true shortest distance.
- **Test recommendation** is reported twice: *known* (confirmed relationships only) and *all*
  (including filename guesses), so the cost of heuristics is visible rather than blended in.
- **Reduction** = 1 − recommended / total test files, the share of the suite *not* run. It is only
  meaningful next to recall: running nothing has 100% reduction.
- **Recall of failing tests** (`historical`) = failing tests that were recommended / failing tests.
  It is reported with a **95% Wilson score interval**, because "found 7 of 8" is far less certain than
  "found 700 of 800". Reference values used by the unit tests: 8/10 → 0.490–0.943; 10/10 → 0.723–1.0;
  0/5 → 0.0–0.435; 1/1 → 0.207–1.0 (computed independently of the Rust code).
- **Completeness rates** — share of files parsed cleanly, share of imports that were unresolved
  (`unresolved / (edges + external + unresolved)`), and whether `test_recommendations.completeness`
  was `complete` — are reported per scenario so that a high score can be read next to how much of
  the code the tool actually understood.

## Synthetic repositories

`SyntheticSpec { files, seed, test_fraction = 0.2, imports_per_module = 3, unresolved_rate = 0.02,
external_rate = 0.05 }`. Modules only import lower-numbered modules (a DAG); the index of an
imported module is drawn as `i·u²` for uniform `u`, which creates a few heavily imported hubs.
Tests import 1–3 random modules. A deterministic PRNG makes every dataset reproducible from
`(files, seed)` on every platform. Impact is checked for the hub (module 0), a middle module and the
last module. The expected impact is computed by a separate reverse breadth-first search over the
generator's own edge list, not by the engine.

## Benchmarking method

Each run is a separate `changegraph analyze` process on a freshly generated repository whose hub
module has an uncommitted edit (the worst case for impact traversal). Wall time is measured around
the process; peak resident memory comes from the kernel's per-child accounting (`wait4`/`ru_maxrss`
on Unix). Min, median and max wall time over `--runs` are reported; the first run is not discarded
(it includes cold file caches, which is what a developer sees). The report also records the
engine's own `execution.duration_ms`, the report size, and graph size. On non-Unix platforms memory
is reported as `n/a`.

Rules for trustworthy numbers: use release builds of both binaries; an otherwise idle machine; the
same machine for before/after comparisons; record the commit. The harness warns when it is a debug
build or the CLI path is not under a `release` directory, but it cannot detect noisy neighbours.
Quote numbers together with the `os`/`arch`/`cpus` stored in the result file.

## Historical evaluation

Build a dataset with `eval/historical/collect-outcomes.mjs` (see `eval/historical/README.md`): it
checks out each commit pair, runs the project's real test suite, and records the test files that
**newly fail** at the later commit. The harness then checks out the later commit in a throwaway
worktree, runs the real analyzer against the earlier commit, and compares the recommendation list
with the recorded failures. It also reports failing tests that the tool does not even recognise as
test files (a naming-convention gap), separately from tests it recognised but did not recommend.

Threats to validity, restated in every report:

- Small samples; read the interval, not the point estimate.
- One project's conventions; a result there says nothing about another project.
- Flaky tests are mitigated (a test counts only if it fails in every head run and no base run) but not
  eliminated.
- "Newly failing" is a proxy for "affected by the change". A change that breaks behaviour no test
  checks is invisible to this experiment.
- Selecting commits where tests failed biases toward interesting cases; document how pairs were chosen.

## Results

No results recorded yet. Template for the table you will fill in from `--json` output (copy the
numbers, do not round away the intervals):

| Date | Machine | Tool commit | Experiment | Result |
|------|---------|-------------|------------|--------|
| *(none)* | | | | |
