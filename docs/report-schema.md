# Report schema (version 0.4.0)

Machine-readable definition: [`schema/report-0.4.0.schema.json`](schema/report-0.4.0.schema.json)
(JSON Schema draft 2020-12). While the major version is `0`, the schema may change between
releases; consumers must check `schema_version`.

| Field | Description |
|-------|-------------|
| `schema_version` | `"0.4.0"` |
| `tool` | `{name, version}` |
| `repository` | `{root, head_commit}` |
| `comparison` | `{mode: "merge_base"\|"working_tree", base, target, base_commit, target_commit, merge_base_commit}`; revision fields are `null` in `working_tree` mode |
| `changed_files[]` | `{path, old_path?, status, category, excluded}`; `status ∈ added, modified, deleted, renamed, copied, type_changed, unmerged, untracked`; `category ∈ source, test, other` |
| `discovery` | `{total_files, source_files, test_files, exclude_patterns[]}` (the file list lives in `graph.nodes`) |
| `graph` | See below |
| `impact` | See below |
| `test_recommendations` | See below |
| `warnings[]` | `{code: "walk_error"\|"dirty_working_tree"\|"parse_error"\|"read_error"\|"tsconfig_issue"\|"graph_target_mismatch"\|"impact_depth_limited"\|"impact_uncertainty"\|"unmodeled_config_change"\|"test_selection_incomplete"\|"coverage_map_issue", message}` |
| `execution` | `{duration_ms}` — wall-clock time and the only non-deterministic field; `null` with `--no-timing` |

### `graph`

`A -> B` means *A imports B*.

| Field | Description |
|-------|-------------|
| `edge_semantics` | Human-readable statement of the edge direction |
| `nodes[]` | `{path, category: source\|test, parse_status: ok\|partial\|failed, exports?[]}` — one per analyzed file, sorted by path |
| `edges[]` | One per **resolved internal** import statement: `{from, to, kind: import\|re_export\|re_export_all, type_only, specifier, line}`. Several edges may connect the same pair |
| `external_dependencies[]` | Imports of packages outside the repo: `{from, package, specifier, kind, type_only, line}`. Never edges |
| `unresolved_imports[]` | Imports that could not be resolved: `{from, specifier, reason: not_found\|outside_repository\|unsupported_file_type\|absolute_path, kind, type_only, line}`. Never edges |
| `cycles[]` | Groups of files that import each other (sorted); includes self-importing files |
| `stats` | Counts, including `parsed_ok`, `parsed_partial`, `parsed_failed` (parse coverage) |

### `impact`

Computed by walking graph edges **in reverse** from the changed files (a change to B can affect
every file that imports B).

| Field | Description |
|-------|-------------|
| `max_depth` / `depth_limited` | The `--max-depth` setting, and whether it cut off further dependents |
| `ordering` | The documented ranking: `distance asc, changed_dependency_count desc, path asc` |
| `changed[]` | `{path, status, category, kind: present\|deleted\|renamed_from, direct_dependents, transitive_dependents}` |
| `affected[]` | Confirmed dependents, ranked: `{rank, path, category, classification: direct\|transitive, distance, changed_dependency_count, affected_by[{changed_file, distance}], path_from_changed[], hops[], explanation}`. `hops[i]` is the import statement `{importer, imported, specifier, line, kind, type_only}` linking `path_from_changed[i]` to `[i+1]` |
| `uncertain[]` | Files **not** reached by the graph whose unresolved imports or incomplete parses may hide a dependency: `{path, category, reasons[{kind: unresolved_import\|parse_partial\|parse_failed, specifier?, line?}]}` |
| `ignored_changed_files[]` | Changed files that were not analyzed, `{path, reason: non_source_file\|excluded\|not_in_graph}` |
| `summary` | `{changed_files, affected, direct, transitive, affected_tests, uncertain}` |

There is deliberately no numeric risk score; `rank` is only a position in the documented ordering.

### `test_recommendations`

Deterministic and explainable; **never a guarantee**. Known relationships are kept apart from
filename guesses.

| Field | Description |
|-------|-------------|
| `coverage_map` | `{path, tests}` if `--coverage-map` was used, else `null` |
| `ordering` | The documented ranking |
| `recommendations[]` | `{rank, path, basis: known\|heuristic, methods[], min_distance, covers[], explanation}`. `methods` ⊆ `changed_test, direct_import, dependency_reachability, coverage_map, filename_heuristic`. `min_distance` is the number of imports to the nearest *changed* module (graph methods only; else `null`) |
| `recommendations[].covers[]` | `{module, module_status: changed\|affected\|removed, method, distance, via?, detail}` — which changed/affected module the test is linked to, by which method, and how |
| `unmapped_modules[]` | Impacted source modules with **no** test linked by a known method: `{path, status, heuristic_tests[]}`. `heuristic_tests` are name-match guesses and do not count as coverage |
| `completeness` | `{complete, reasons[{kind, detail}]}`; `kind ∈ unmapped_modules, depth_limited, uncertain_files, unmodeled_config_change, unanalyzed_changed_files, graph_target_mismatch, no_tests_recommended`. When `complete` is `false` the list **must not** be treated as the full set of tests to run |
| `summary` | `{total_test_files, recommended, known_basis, heuristic_only}` |

An empty `recommendations` list is never evidence that no tests are needed: for a real change
it comes with `completeness.reasons` containing `no_tests_recommended`.

### Coverage map input (`--coverage-map FILE`)

```json
{ "version": 1, "tests": { "tests/a.test.ts": ["src/a.ts", "src/b.ts"] } }
```

Paths are repository-relative. The map is user-supplied (e.g. per-test coverage reduced to the
source files each test executed). It is not checked against the revision being analyzed.

**Changes from 0.3.0:** added `test_recommendations`; `execution.duration_ms` may be `null`; two
warning codes. **Changes from 0.2.0:** added `impact`, four warning codes. **Changes from 0.1.0 (earlier):** added `graph`; removed `discovery.files` (use `graph.nodes`); added warning codes.

Everything else is identical for identical repository state and configuration.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success (warnings do not change the exit code) |
| 1 | Internal, I/O, or serialization error |
| 2 | Usage / invalid configuration (e.g. `--target` without `--base`, `--compact` without `--format json`, malformed `--coverage-map`) |
| 3 | Path (including a `--coverage-map` file) does not exist, is not a directory, or is not a Git repository |
| 4 | Revision unknown, invalid, or no merge base |
| 5 | `git` missing, failed, or produced unparseable output |
| 6 | Analysis **succeeded** but a `--fail-on` condition was met (`warnings`, `incomplete-tests`); the report is still written |
