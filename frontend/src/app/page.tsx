"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import type { DragEvent } from "react";
import { EmptyState, ErrorPanel, FileButton } from "../components/Notices";
import { FileLists } from "../components/FileLists";
import { FilterBar } from "../components/FilterBar";
import { GraphView } from "../components/GraphView";
import { Inspector } from "../components/Inspector";
import { IntegrityStrip } from "../components/IntegrityStrip";
import { Legend } from "../components/Legend";
import { SummaryPanel } from "../components/SummaryPanel";
import { defaultFilters, matchPaths, visibleEdges, visiblePaths } from "../lib/filters.ts";
import type { Filters } from "../lib/filters.ts";
import { describeComparison } from "../lib/format.ts";
import { integrityNotes } from "../lib/integrity.ts";
import { layeredLayout } from "../lib/layout.ts";
import { buildModel, impactPathEdgeIds, impactPathNodes } from "../lib/model.ts";
import type { ReportModel } from "../lib/model.ts";
import { parseReport } from "../lib/validate-report.ts";

interface Loaded {
  model: ReportModel;
  label: string;
  bundled: boolean;
}

export default function Page() {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [errors, setErrors] = useState<string[] | null>(null);
  const [busy, setBusy] = useState(true);
  const [filters, setFilters] = useState<Filters | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [showPath, setShowPath] = useState(true);
  const [dragging, setDragging] = useState(false);

  const load = useCallback((text: string, label: string, bundled: boolean) => {
    const result = parseReport(text);
    if (!result.ok) {
      setErrors(result.errors);
      return;
    }
    const model = buildModel(result.report);
    setLoaded({ model, label, bundled });
    setFilters(defaultFilters(model));
    setSelected(null);
    setErrors(null);
  }, []);

  const openFile = useCallback(
    async (file: File) => {
      try {
        load(await file.text(), file.name, false);
      } catch (e) {
        setErrors([`Could not read ${file.name}: ${e instanceof Error ? e.message : String(e)}`]);
      }
    },
    [load],
  );

  useEffect(() => {
    let alive = true;
    fetch("/sample-report.json")
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((text) => {
        if (alive) load(text, "sample-report.json", true);
      })
      .catch(() => {
        /* no bundled sample: the empty state explains how to open a report */
      })
      .finally(() => {
        if (alive) setBusy(false);
      });
    return () => {
      alive = false;
    };
  }, [load]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setSelected(null);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const model = loaded?.model ?? null;

  const pathNodes = useMemo(
    () => new Set(model && selected && showPath ? impactPathNodes(model, selected) : []),
    [model, selected, showPath],
  );
  const pathEdges = useMemo(
    () => new Set(model && selected && showPath ? impactPathEdgeIds(model, selected) : []),
    [model, selected, showPath],
  );
  const forced = useMemo(() => {
    const s = new Set(pathNodes);
    if (selected) s.add(selected);
    return s;
  }, [pathNodes, selected]);

  const visible = useMemo(() => (model && filters ? visiblePaths(model, filters, forced) : []), [model, filters, forced]);
  const edges = useMemo(() => (model ? visibleEdges(model, new Set(visible)) : []), [model, visible]);
  const layout = useMemo(() => layeredLayout(visible, edges), [visible, edges]);
  const matches = useMemo(() => (model && filters ? matchPaths(model, filters.query) : new Set<string>()), [model, filters]);
  const notes = useMemo(() => (model ? integrityNotes(model) : []), [model]);

  const onDrop = (e: DragEvent<HTMLDivElement>) => {
    e.preventDefault();
    setDragging(false);
    const f = e.dataTransfer.files[0];
    if (f) void openFile(f);
  };

  return (
    <div
      className="relative flex h-dvh flex-col"
      onDragOver={(e) => {
        e.preventDefault();
        setDragging(true);
      }}
      onDragLeave={(e) => {
        if (e.currentTarget === e.target) setDragging(false);
      }}
      onDrop={onDrop}
    >
      <header className="flex flex-wrap items-center gap-x-4 gap-y-2 border-b border-ink-line bg-ink-panel px-4 py-2">
        <span className="text-base font-semibold">ChangeGraph</span>
        {model && loaded && (
          <div className="min-w-0 flex-1 text-sm">
            <p className="truncate">{describeComparison(model.report)}</p>
            <p className="truncate text-xs text-ink-dim">
              {loaded.bundled
                ? "Bundled sample for fixtures/simple-ts-project (path and commit are placeholders); `cargo test golden` checks it against the analyzer's real output."
                : `${loaded.label} · changegraph ${model.report.tool.version} · schema ${model.report.schema_version}`}
            </p>
          </div>
        )}
        <div className="ml-auto">
          <FileButton onFile={(f) => void openFile(f)} label={model ? "Open another report" : "Open report"} />
        </div>
      </header>

      {errors && <ErrorPanel errors={errors} onDismiss={() => setErrors(null)} />}

      {model && filters ? (
        <>
          <IntegrityStrip notes={notes} />
          <main className="flex min-h-0 flex-1 flex-col overflow-y-auto lg:grid lg:grid-cols-[300px_minmax(0,1fr)_380px] lg:overflow-hidden">
            <aside className="border-ink-line lg:overflow-y-auto lg:border-r">
              <SummaryPanel model={model} />
              <FilterBar model={model} filters={filters} matchCount={matches.size} onChange={setFilters} />
              <FileLists model={model} selected={selected} onSelect={setSelected} />
            </aside>

            <section aria-label="Dependency graph" className="relative h-[70vh] min-h-[420px] lg:h-auto">
              <GraphView
                model={model}
                visible={visible}
                edges={edges}
                layout={layout}
                selected={selected}
                pathNodes={pathNodes}
                pathEdges={pathEdges}
                matches={matches}
                queryActive={filters.query.trim() !== ""}
                onSelect={setSelected}
              />
              <div className="pointer-events-none absolute left-3 top-3 flex flex-col gap-2">
                <p className="pointer-events-auto rounded border border-ink-line bg-ink-panel/95 px-2 py-1 text-xs text-ink-dim" aria-live="polite">
                  Showing {visible.length} of {model.order.length} files
                </p>
                <Legend />
              </div>
            </section>

            <aside aria-label="File details" className="border-t border-ink-line lg:overflow-y-auto lg:border-l lg:border-t-0">
              <Inspector model={model} path={selected} showPath={showPath} onShowPath={setShowPath} onSelect={setSelected} />
            </aside>
          </main>
        </>
      ) : (
        <div className="min-h-0 flex-1 overflow-y-auto">
          <EmptyState onFile={(f) => void openFile(f)} busy={busy} />
        </div>
      )}

      {dragging && (
        <div className="pointer-events-none absolute inset-0 z-50 flex items-center justify-center border-4 border-dashed border-signal-test bg-ink/80 text-lg">
          Drop a report.json to open it
        </div>
      )}
    </div>
  );
}
