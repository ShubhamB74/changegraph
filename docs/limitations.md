# Known limitations

**Current (Phase 1)**

- No dependency graph, impact analysis, or test recommendations exist yet.
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
