# Supported syntax

What ChangeGraph understands, what it understands only partly, and what it ignores. "Evidence" names
the test (or fixture) that checks the row; a row with no evidence is stated from reading the code
and is the first place to look if behaviour surprises you.

The graph is built from **syntax only** (Tree-sitter). It does not type-check, run a bundler, or
follow the TypeScript compiler's full module resolution.

## Files

| Item | Behaviour | Evidence |
|------|-----------|----------|
| Analyzed extensions | `.ts .tsx .js .jsx .mjs .cjs .mts .cts` (and `.d.ts`) | `discovery::tests::classifies_files` |
| Grammar | `.ts/.mts/.cts` → TypeScript; `.tsx` → TSX; `.js/.jsx/.mjs/.cjs` → JavaScript (parses JSX) | `parser::tests::language_from_extension`, `javascript_and_tsx_grammars_work` |
| Test files | name ends in `.test.<ext>` or `.spec.<ext>`, or the path contains a `__tests__` directory. A file under `tests/` is **not** a test unless it matches one of these. | `discovery::tests::classifies_files` |
| Excluded by default | `node_modules .git dist build out coverage .next .turbo` (any path component), plus `--exclude` patterns | `discovery::tests::exclude_*`, `tests/discovery.rs` |
| Ignore files | `.gitignore` files **inside** the tree are honoured; global and `.git/info/exclude` are deliberately not | `tests/discovery.rs` |
| Syntax errors | the file is `partial`: facts come from the top-level statements that parsed; a warning gives the first error line | `parser::tests::syntax_errors_yield_partial_status_with_line`, `edge-cases-project/src/broken.ts` |

## Imports and re-exports (top-level statements)

| Form | Result | Evidence |
|------|--------|----------|
| `import a from "x"`, `import {a as b} from "x"`, `import * as n from "x"` | edge, kind `import` | `parser::tests::extracts_static_imports` |
| `import "x"` (side effect) | edge, kind `import` | `extracts_static_imports`; `edge-cases-project` (`./side-effect`) |
| `import type {A} from "x"`, `import type A from "x"` | edge, `type_only: true` | `detects_statement_level_type_only_imports`; `edge-cases-project` (`./types`) |
| `import type from "x"` (a default import *named* `type`) | edge, `type_only: false` | `type_modifier_helper_distinguishes_default_import_named_type` |
| `import {type A} from "x"` (per-specifier) | edge, **`type_only: false`** (treated as a runtime import) | `detects_statement_level_type_only_imports` |
| `export {a} from "x"` | edge, kind `re_export` | `extracts_reexports`; `edge-cases-project` |
| `export * from "x"`, `export * as ns from "x"` | edge, kind `re_export_all` | `extracts_reexports`; `edge-cases-project` |
| `export type {T} from "x"` | edge, kind `re_export`, `type_only: true` | `extracts_reexports` |

Every edge is **file to file**. Importing one name links the whole file; the graph has no symbol-level
edges, so it can over-approximate impact.

## Local exports (recorded in `graph.nodes[].exports`, best effort; they do not create edges)

Recorded: `function`, generator, `class`, `abstract class`, `interface`, `type`, `enum`, `const/let/var`
with plain identifier names, `export {a, b as c}` (the exported name), and `export default` (as `default`).
Not recorded: destructuring patterns, `export declare ...`, `export namespace`/`module`, `export import`.
Evidence: `extracts_local_export_names`, `plain_declarations_are_not_exports`.

## Not extracted (dependencies through these are invisible)

- dynamic `import("x")` and `require("x")`
- `import x = require("x")` and `export = x`
- imports nested inside `declare module "..." { }` blocks
- `/// <reference path="...">` directives
- `package.json` `exports` / `imports` maps and workspace packages (`@org/pkg` resolved through
  `node_modules` symlinks appear as **external** packages)
- non-TypeScript/JavaScript files: Vue/Svelte single-file components, CSS/SCSS, JSON, images. An import
  of such an asset is reported as unresolved with reason `unsupported_file_type`.

Code that relies on these will have its real dependencies under-reported. `impact.uncertain` only
covers gaps the tool *detected* (unresolved imports, partial parses), not undetected ones.

## Module resolution

Order for a specifier imported from file `F` (authoritative text: the `resolver` module docs):

1. `node:` prefix → external.
2. Leading `/` → unresolved (`absolute_path`).
3. Relative (`./`, `../`, `.`, `..`) → join with `F`'s directory, normalise, then probe.
4. `tsconfig.json` `paths`: longest matching pattern wins; targets tried in order. A matched alias
   that finds no file is **unresolved**, never silently external.
5. `tsconfig.json` `baseUrl`: bare specifier probed against it.
6. Otherwise → external package.

Probe order for a path `P`: `P.js → P.ts, P.tsx` (likewise `.jsx → .tsx`, `.mjs → .mts`, `.cjs → .cts`);
`P` exactly; `P` + `.ts .tsx .d.ts .mts .cts .js .jsx .mjs .cjs`; then `P/index` + the same extensions.
Only analyzed files can be targets, so a relative import into an excluded directory is unresolved.

Evidence: `relative_resolution_extensions_and_index`, `typescript_style_js_extension_maps_to_ts`,
`misses_are_unresolved_with_reasons`, `bare_specifiers_are_external_not_internal`,
`alias_resolution_prefers_longest_prefix_and_exact_match`,
`matched_alias_without_file_is_unresolved_not_external`, `base_url_resolves_bare_specifiers_else_external`.

## `tsconfig.json`

| Feature | Support | Evidence |
|---------|---------|----------|
| Root `tsconfig.json` only | yes | `tsconfig` tests |
| Comments and trailing commas (JSONC) | yes | `strips_comments_and_trailing_commas_but_not_inside_strings` |
| `compilerOptions.baseUrl`, `compilerOptions.paths` | yes (one `*` per pattern) | `parses_base_url_and_paths` |
| `extends` | **not followed**; a `tsconfig_issue` warning is emitted | `extends_is_flagged_and_missing_options_are_fine` |
| project `references`, `rootDirs`, multiple tsconfig files | no | |
| Malformed file | alias resolution disabled, `tsconfig_issue` warning | `malformed_json_is_an_error` |
