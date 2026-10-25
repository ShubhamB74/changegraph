# ChangeGraph viewer

A static web page that opens a `changegraph analyze --format json` report and lets you explore it:
which files a change reaches, the exact import chain behind each one, and which tests are (and are
not) linked to it. It runs entirely in the browser; the report is never uploaded.

```bash
cd frontend
npm install
npm run dev            # http://localhost:3000, opens the bundled sample
```

Open your own report with **Open report** or by dropping the file on the page:

```bash
changegraph analyze . --base main --format json -o report.json
```

## Rules the viewer follows

- **It never invents relationships.** Every node, edge and annotation comes from the report. A report
  that contradicts itself (an edge to a file that is not listed, an impact path that does not end at
  its file) is rejected, not drawn. Removed files appear as dashed "ghost" nodes only because the
  report lists them as changed files and as the first step of impact paths.
- **It never hides incomplete analysis.** The strip under the header lists what the report cannot
  tell you (partial parses, unresolved imports, uncertain files, truncated depth, incomplete test
  selection, graph/target mismatch) and is always visible. The inspector says "No test is linked to
  this file by a known method" rather than staying silent.
- **It only opens the schema it was written for.** The report is validated against
  `docs/schema/report-0.4.0.schema.json` (embedded as `src/lib/report-schema.ts`). Another version
  gets a clear message instead of a half-rendered page.

## Reading the graph

An arrow points from the importing file to the file it imports (`A -> B`: A imports B). Files sit in
columns by import depth, so impact reads left to right. Border colour and style (see the legend)
carry the state: changed, removed, direct dependent, transitive dependent, uncertain, not affected.
Select a file to see why it is affected, the import chain with the exact statement and line, and
its tests; the chain is highlighted on the graph and always kept visible even if filters would hide
part of it. Keyboard: every file is also reachable from the "Affected files" and "Recommended tests"
lists; `Esc` clears the selection.

**Impact only** (default) shows changed, affected, removed and uncertain files. **All files** shows
the whole graph; above 1,500 visible files the page says so and suggests narrowing. Search matches
any path, including files outside the current scope.

## Layout of the code

| Path | Purpose |
|------|---------|
| `src/lib/validate-report.ts` | Schema + referential-integrity validation |
| `src/lib/model.ts` | Report → view model (states, adjacency, test links) |
| `src/lib/filters.ts` | Scope, state filters, search |
| `src/lib/layout.ts` | Deterministic, dependency-free layered layout |
| `src/lib/flow-graph.ts` | What the canvas draws (pure; unit-tested) |
| `src/lib/integrity.ts` | The "what this report cannot tell you" notes |
| `src/components/*` | React UI; `GraphView`/`FileNode` wrap React Flow (`@xyflow/react`) |
| `public/sample-report.json` | Sample so the UI previews immediately |

## The bundled sample

`public/sample-report.json` is **hand-assembled** from the labeled expectations for
`fixtures/simple-ts-project` (graph, impact, tests) because it was written without a Rust toolchain.
The UI says so in the header. Replace it with real engine output:

```bash
npm run sample      # runs the real analyzer on a scratch copy of the fixture
npm test            # confirms the viewer accepts the real report
```

## Checks

```bash
npm test            # 39 tests on the logic layer (Node >= 22.6, built-in test runner)
npm run typecheck
npm run lint
npm run build       # static export to out/
```

After the engine changes its report schema: `npm run sync-schema`, then update
`SUPPORTED_SCHEMA_VERSION` in `src/lib/report-types.ts`. `schema-sync.test.ts` fails if they drift.

## Known limitations

- Static imports only, as in the engine; "uncertain" covers only gaps the engine detected.
- Layout is layered, not force-directed, and recomputed on every filter change. It is fast on the
  tested synthetic graph (1,500 files / 6,000 imports) but very large "All files" views are dense.
- The viewer reads the whole report into memory; there is no streaming for very large reports.
- Node dragging is disabled on purpose: positions are derived, so moving a node would not mean
  anything.
- There is no server, history or comparison between two reports.
