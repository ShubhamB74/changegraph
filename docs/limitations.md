# Known limitations

**Current (Phase 1)**

- Test recommendations do not exist yet (Phase 4); `affected` simply includes affected test files.
- **Impact is file-level.** Changing one function marks every importer of the file as affected,
  whether or not it uses that function. Expect over-approximation (extra files), not precision.
- **Impact only follows static imports.** Dependencies via dynamic `import()`, `require()`, DI
  containers, global state, build configuration, generated code, or runtime data are invisible,
  so real impact can be larger than reported. `uncertain` only covers *detected* gaps
  (unresolved imports, partial parses), not undetected ones.
- Changes to `package.json`, lockfiles or tsconfig can affect many files; this is flagged with a
  warning but not modelled.
- The graph reflects the files on disk, not necessarily the target revision (see
  `graph_target_mismatch`). Building the graph from git objects instead is future work.
- Rename detection depends on git's similarity threshold; a heavily edited rename appears as a
  deletion plus an addition (the deletion's importers are still flagged).
- `affected_by` stores every (changed file, node) pair; with very many changed files on a very
  large graph this grows as seeds x reachable nodes.
- The ranking is an ordering heuristic, not a probability of breakage.
- The graph is built from **syntax only**. No type information is used, so e.g. a `.d.ts`
  declaration that shadows a module, or types re-exported through ambient modules, are not modelled.
- Not extracted: dynamic `import()`, `require()`, `import x = require()`, `export =`, imports
  inside `declare module` blocks, `/// <reference>` directives, and `package.json` `exports`/
  `imports` maps. Code relying on these will under-report dependencies.
- Per-specifier `import { type A }` is treated as a runtime import (`type_only: false`).
- Edges are file-level, not symbol-level: importing one name from a file links the whole file.
- tsconfig support: root `tsconfig.json` only; `extends`, project references and `rootDirs` are
  not followed (a warning is emitted for `extends`). Only a single `*` wildcard per pattern.
- Monorepo workspace packages (`@org/pkg` resolved via node_modules symlinks) appear as
  *external* dependencies, not internal edges.
- Partial parses extract imports only from statements that parsed; error recovery is
  grammar-dependent, so imports near a syntax error may be missed (the file is flagged `partial`).
- Exclusions do not support globs.
- If the target path is a subdirectory of a repository, the whole repository is analyzed.
- Non-UTF-8 file names are converted lossily and could collide.
- Submodules are not traversed specially. Shallow clones may lack the base revision or a merge
  base; the tool reports this rather than guessing.
- Windows paths are normalized to `/`, but the project has not been exercised on Windows.

**Standing claims we will not make**

- Tree-sitter gives syntax, not TypeScript semantics; the tool will not claim full semantic
  analysis.
- Dependency reachability or filename similarity does not prove a test catches a regression.
- Impact rankings are heuristics, not probabilities. Safe test omission requires empirical
  validation (Phase 7) and is not promised.
