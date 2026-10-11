# Report schema (version 0.1.0)

Machine-readable definition: [`schema/report-0.1.0.schema.json`](schema/report-0.1.0.schema.json)
(JSON Schema draft 2020-12). While the major version is `0`, the schema may change between
releases; consumers must check `schema_version`.

| Field | Description |
|-------|-------------|
| `schema_version` | `"0.1.0"` |
| `tool` | `{name, version}` |
| `repository` | `{root, head_commit}` |
| `comparison` | `{mode: "merge_base"\|"working_tree", base, target, base_commit, target_commit, merge_base_commit}`; revision fields are `null` in `working_tree` mode |
| `changed_files[]` | `{path, old_path?, status, category, excluded}`; `status ∈ added, modified, deleted, renamed, copied, type_changed, unmerged, untracked`; `category ∈ source, test, other` |
| `discovery` | `{total_files, source_files, test_files, exclude_patterns[], files[{path, category}]}` |
| `warnings[]` | `{code: "walk_error"\|"dirty_working_tree", message}` |
| `execution` | `{duration_ms}` — the only non-deterministic field |

Everything else is identical for identical repository state and configuration.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Success (warnings do not change the exit code) |
| 1 | Internal, I/O, or serialization error |
| 2 | Usage / invalid configuration (e.g. `--target` without `--base`) |
| 3 | Path does not exist, is not a directory, or is not a Git repository |
| 4 | Revision unknown, invalid, or no merge base |
| 5 | `git` missing, failed, or produced unparseable output |
