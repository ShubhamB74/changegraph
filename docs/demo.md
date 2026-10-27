# Demonstration

A five-minute walkthrough on the bundled fixture. Everything here runs the real tools.

```bash
scripts/demo.sh demo-report.json
```

The script copies `fixtures/simple-ts-project` into a scratch Git repository, appends a comment to
`src/auth/session.ts` (left uncommitted), prints the analyzer's human-readable report, and writes the JSON
report to `demo-report.json`.

## What to look for in the terminal output

1. **Changed files**: one file, `src/auth/session.ts`, modified.
2. **Impact**: four affected files, three direct (`src/api/profile.ts`, `src/index.ts`,
   `tests/session.test.ts`) and one transitive (`tests/profile.test.ts`, two imports away). Each line says
   *why*: the importing file, the specifier and the line number.
3. **Recommended tests**: `tests/session.test.ts` (direct import) and `tests/profile.test.ts` (reaches the
   change through `profile.ts`), both marked as a *known* relationship.
4. **The honest part**: `src/index.ts` is affected but no test imports it, so the tool lists it as having
   *no test linked by a known method* and prints **TEST SELECTION IS INCOMPLETE**. It does not claim the two
   recommended tests are sufficient.

## In the viewer

```bash
cd frontend && npm install && npm run dev      # http://localhost:3000
```

Choose `demo-report.json` with **Open report** (or drop it on the page). Then:

1. The strip under the header states the limits of the analysis; it is always visible.
2. Select `tests/profile.test.ts`. The inspector shows the explanation and the import chain
   `session.ts → profile.ts → profile.test.ts` with each statement and line; the chain is highlighted.
3. Select `src/index.ts`. The inspector warns that no test is linked to it.
4. Tick **All files**, search for `session`, untick **Show test files**: filters never cut a highlighted
   chain in half.

## Variations worth trying

- `changegraph analyze <repo> --max-depth 1`: the report says the traversal was cut off, and the test
  selection becomes incomplete.
- Delete `src/auth/session.ts` instead of editing it: its importers are reported as affected through the
  imports that no longer resolve (shown dashed in the viewer).
- `--fail-on incomplete-tests` exits with code 6 when the selection is incomplete, for use in CI.

Screenshots are not committed yet; see `docs/screenshots/README.md`.
