# Expected outputs

Hand-labeled expectations used by `crates/changegraph-core/tests/graph.rs` (and, in Phase 7,
the evaluation harness). `A -> B` means A imports B.

| File | Fixture | Covers |
|------|---------|--------|
| `simple-ts-project.graph.json` | `fixtures/simple-ts-project` | imports, re-exports, tests importing sources |
| `edge-cases-project.graph.json` | `fixtures/edge-cases-project` | `.js`→`.ts` mapping, directory `index`, tsconfig alias (JSONC), type-only import, side-effect import, `export *`, external packages, unresolved (`not_found`, `unsupported_file_type`), an import cycle, a syntax error (`partial`) |

These are labels written by hand from reading the fixture sources, not output copied from the tool.
`src/broken.ts` is excluded from edge comparison because error recovery is grammar-dependent;
only its `partial` status is asserted.

## Impact scenarios

`*.impact.json` files contain hand-labeled scenarios for Phase 3: a list of changed files and the
expected `affected` list **in ranked order** (`path`, `classification`, `distance`,
`changed_dependency_count`), optionally `uncertain` (exact) or `*_includes` (subset) checks.
Ranking: distance asc, changed_dependency_count desc, path asc.
