import type { ImpactState, PairEdge, ReportModel } from "./model.ts";

export type Scope = "impact" | "all";

export interface Filters {
  scope: Scope;
  states: Record<ImpactState, boolean>;
  showTests: boolean;
  query: string;
}

export const ALL_STATES: ImpactState[] = ["changed", "removed", "direct", "transitive", "uncertain", "unaffected"];

export function defaultFilters(model: ReportModel): Filters {
  return {
    // With nothing impacted, an empty "impact" scope would look like a bug; show everything.
    scope: model.impactedCount > 0 ? "impact" : "all",
    states: { changed: true, removed: true, direct: true, transitive: true, uncertain: true, unaffected: true },
    showTests: true,
    query: "",
  };
}

export function matchPaths(model: ReportModel, query: string): Set<string> {
  const q = query.trim().toLowerCase();
  const out = new Set<string>();
  if (!q) return out;
  for (const p of model.order) if (p.toLowerCase().includes(q)) out.add(p);
  return out;
}

/**
 * Paths to draw, sorted. `forced` (e.g. the expanded impact path of the selected file) is always
 * included so a path can never be cut in half by a filter.
 */
export function visiblePaths(model: ReportModel, filters: Filters, forced: ReadonlySet<string> = new Set()): string[] {
  const matches = matchPaths(model, filters.query);
  const out = new Set<string>();
  for (const p of model.order) {
    const n = model.nodes.get(p);
    if (!n) continue;
    const inScope = filters.scope === "all" || n.state !== "unaffected";
    if (!inScope && !matches.has(p)) continue;
    if (!filters.states[n.state]) continue;
    if (!filters.showTests && n.category === "test") continue;
    out.add(p);
  }
  for (const p of forced) if (model.nodes.has(p)) out.add(p);
  return [...out].sort();
}

export function visibleEdges(model: ReportModel, visible: ReadonlySet<string>): PairEdge[] {
  return model.edges.filter((e) => visible.has(e.from) && visible.has(e.to));
}
