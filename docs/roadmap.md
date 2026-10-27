# Roadmap

Nothing on this page is implemented. It exists so that the next step is chosen deliberately.
Non-goals stay fixed: no LLM in the analysis engine, no claim of full semantic analysis, no safe test
omission without empirical validation.

## 1. Before anything else: measure

The evaluation harness exists (`docs/evaluation.md`) but no results are recorded. The highest-value
next step is to build one or more historical datasets on real repositories and record the
`historical`, `scenarios`, `synthetic` and `bench` output. Every item below should be justified by
what those numbers show (for example, a high rate of failing tests the tool does not recognise as
tests argues for configurable test patterns before it argues for a new language).

## 2. Engine improvements for TypeScript/JavaScript

| Item | Why | Notes |
|------|-----|-------|
| Build the graph from Git objects, not the working tree | Fixes the `graph_target_mismatch` limitation; makes `--target` exact | Read blobs via `git cat-file`/`ls-tree`; the resolver already works on a path set |
| Monorepo workspaces | `@org/pkg` imports are currently *external* | Read `package.json` `workspaces`, resolve to member package entry points |
| Follow `tsconfig` `extends` | Aliases defined in a parent config are lost today | Needs a merge order that matches `tsc` |
| Dynamic `import()` and `require()` with literal arguments | Common in CommonJS and lazy-loaded code | Record as edges of a distinct kind; keep non-literal ones as uncertain |
| Per-specifier `type` imports | `import {type A}` is currently a runtime edge | Mixed statements need per-specifier edge kinds |
| Symbol-level impact | Removes the file-level over-approximation | Large: requires reasonably faithful name resolution through re-exports |
| Configurable test-file patterns | Tests with other naming are invisible today | Config file, not more hard-coded heuristics |
| Per-test coverage importers | The coverage map is hand-supplied today | lcov/Istanbul/V8 per-test conversion tools |
| Incremental analysis / caching | Re-parsing everything on each run | Only if `bench` shows it matters |
| CI integrations | Make results visible where decisions happen | SARIF or PR-comment output; the JSON report is already the stable interface |

## 3. Supporting other languages

The engine's stages are language-independent in shape (discover files → extract imports → resolve →
graph → impact → tests), but today the TypeScript/JavaScript knowledge is spread across
`discovery` (extensions, test names), `parser`, `resolver` and `tsconfig`. Adding a language means
first making that boundary explicit.

**Proposed boundary (design sketch).** A language adapter provides: the file extensions it owns; a
test-file predicate; `extract(source) -> {imports, exports, parse status}`; and a resolver from
`(importer, specifier)` to *internal file / external package / unresolved(reason)*. The graph, impact,
test-mapping, report and viewer would not change: the report already speaks in files, edges and
reasons. The `language` of each node would need to be added to the schema.

| Language | What makes it harder than TypeScript | Starting point |
|----------|--------------------------------------|----------------|
| Python | Imports are dynamic and the search path is runtime state; namespace packages; `__init__.py` re-exports; relative imports depend on package layout | `tree-sitter-python`; resolve against detected package roots (`pyproject.toml`, `src/` layout); treat `importlib` use as uncertain |
| Go | Dependencies are *packages* (directories), not files; module paths come from `go.mod`; build tags | Package-level nodes; `go list -deps -json` is the reliable source |
| Java / Kotlin | Dependencies are classes by fully qualified name; wildcard imports; classpath and annotation processing | Map package → directories; accept lower precision for wildcards |
| Rust | `mod` trees plus the crate graph | Use `cargo metadata` for crates and parse `mod`/`use` for modules |
| C# | Namespaces do not correspond to files | Project-level graph first (`.csproj` references) |

Criteria for taking one on: a concrete user with a repository and a test suite to evaluate on; the
historical evaluation run for that language *before* anything is claimed; and an honest
`docs/supported-syntax.md` equivalent. A language whose dependencies cannot be recovered statically
with reasonable fidelity should say so rather than ship a graph that looks authoritative.

## 4. Product and documentation

- Screenshots and a recorded walkthrough (see `docs/screenshots/README.md`).
- A published JSON Schema URL instead of the placeholder `$id`.
- A frontend lockfile and `npm ci` in CI; a Rust `Cargo.lock` committed and `--locked` in CI.
