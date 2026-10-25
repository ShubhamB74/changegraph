// Turns a validated report into the view model the UI renders.
// Rule: every node, edge and annotation comes from the report. Nothing is inferred or invented.
// (Deleted/renamed-away files are drawn as "removed" ghost nodes because the report lists them
// as changed files and as the first hop of impact paths.)

import type {
  AffectedNode,
  ChangeStatus,
  ChangedNode,
  DependencyEdge,
  ExternalDependency,
  ParseStatus,
  Report,
  TestRecommendation,
  UncertainNode,
  UnresolvedImport,
} from "./report-types.ts";

export type ImpactState = "changed" | "direct" | "transitive" | "uncertain" | "unaffected" | "removed";

export interface FileNodeInfo {
  path: string;
  category: "source" | "test";
  state: ImpactState;
  /** null for removed (ghost) files, which are not parsed. */
  parseStatus: ParseStatus | null;
  changeStatus: ChangeStatus | null;
  distance: number | null;
  rank: number | null;
  exports: string[];
  changed: ChangedNode | null;
  affected: AffectedNode | null;
  uncertain: UncertainNode | null;
  dependsOn: string[];
  dependedOnBy: string[];
  unresolved: UnresolvedImport[];
  external: ExternalDependency[];
  /** Recommended tests linked to this module. */
  coveredBy: TestRecommendation[];
  /** Set when this file is itself a recommended test. */
  recommendation: TestRecommendation | null;
  ghost: boolean;
}

export interface PairEdge {
  id: string;
  from: string;
  to: string;
  statements: DependencyEdge[];
  /** True when the target is a removed file (the import no longer resolves). */
  ghost: boolean;
}

export interface ReportModel {
  report: Report;
  nodes: Map<string, FileNodeInfo>;
  /** Sorted paths of every node, including ghosts. */
  order: string[];
  edges: PairEdge[];
  edgesById: Map<string, PairEdge>;
  impactedCount: number;
}

export const edgeId = (from: string, to: string): string => `${from} -> ${to}`;

export function buildModel(report: Report): ReportModel {
  const nodes = new Map<string, FileNodeInfo>();
  for (const n of report.graph.nodes) {
    nodes.set(n.path, {
      path: n.path,
      category: n.category,
      state: "unaffected",
      parseStatus: n.parse_status,
      changeStatus: null,
      distance: null,
      rank: null,
      exports: n.exports ?? [],
      changed: null,
      affected: null,
      uncertain: null,
      dependsOn: [],
      dependedOnBy: [],
      unresolved: [],
      external: [],
      coveredBy: [],
      recommendation: null,
      ghost: false,
    });
  }

  const ghostPaths = new Set<string>();
  for (const c of report.impact.changed) {
    const existing = nodes.get(c.path);
    if (c.kind === "present" && existing) {
      existing.state = "changed";
      existing.changeStatus = c.status;
      existing.distance = 0;
      existing.changed = c;
    } else if (c.kind !== "present" && !existing) {
      ghostPaths.add(c.path);
      nodes.set(c.path, {
        path: c.path,
        category: c.category === "test" ? "test" : "source",
        state: "removed",
        parseStatus: null,
        changeStatus: c.status,
        distance: 0,
        rank: null,
        exports: [],
        changed: c,
        affected: null,
        uncertain: null,
        dependsOn: [],
        dependedOnBy: [],
        unresolved: [],
        external: [],
        coveredBy: [],
        recommendation: null,
        ghost: true,
      });
    }
  }

  for (const a of report.impact.affected) {
    const n = nodes.get(a.path);
    if (!n || n.state === "changed") continue;
    n.state = a.classification;
    n.distance = a.distance;
    n.rank = a.rank;
    n.affected = a;
  }
  for (const u of report.impact.uncertain) {
    const n = nodes.get(u.path);
    if (!n || n.state !== "unaffected") continue;
    n.state = "uncertain";
    n.uncertain = u;
  }

  for (const u of report.graph.unresolved_imports) nodes.get(u.from)?.unresolved.push(u);
  for (const x of report.graph.external_dependencies) nodes.get(x.from)?.external.push(x);

  const pairs = new Map<string, PairEdge>();
  for (const e of report.graph.edges) {
    const id = edgeId(e.from, e.to);
    const pair = pairs.get(id);
    if (pair) pair.statements.push(e);
    else pairs.set(id, { id, from: e.from, to: e.to, statements: [e], ghost: false });
  }
  // Imports of removed files: taken from the report's own impact hops.
  for (const a of report.impact.affected) {
    for (const h of a.hops) {
      if (!ghostPaths.has(h.imported)) continue;
      const id = edgeId(h.importer, h.imported);
      if (pairs.has(id)) continue;
      pairs.set(id, {
        id,
        from: h.importer,
        to: h.imported,
        ghost: true,
        statements: [
          { from: h.importer, to: h.imported, kind: h.kind, type_only: h.type_only, specifier: h.specifier, line: h.line },
        ],
      });
    }
  }

  const edges = [...pairs.values()].sort((x, y) => (x.id < y.id ? -1 : x.id > y.id ? 1 : 0));
  for (const e of edges) {
    const from = nodes.get(e.from);
    const to = nodes.get(e.to);
    if (from && !from.dependsOn.includes(e.to)) from.dependsOn.push(e.to);
    if (to && !to.dependedOnBy.includes(e.from)) to.dependedOnBy.push(e.from);
  }
  for (const n of nodes.values()) {
    n.dependsOn.sort();
    n.dependedOnBy.sort();
  }

  for (const rec of report.test_recommendations.recommendations) {
    const t = nodes.get(rec.path);
    if (t) t.recommendation = rec;
    const seen = new Set<string>();
    for (const c of rec.covers) {
      if (seen.has(c.module)) continue;
      seen.add(c.module);
      nodes.get(c.module)?.coveredBy.push(rec);
    }
  }

  const order = [...nodes.keys()].sort();
  let impactedCount = 0;
  for (const n of nodes.values()) {
    if (n.state !== "unaffected") impactedCount++;
  }
  return {
    report,
    nodes,
    order,
    edges,
    edgesById: new Map(edges.map((e) => [e.id, e])),
    impactedCount,
  };
}

export const STATE_LABEL: Record<ImpactState, string> = {
  changed: "Changed",
  direct: "Direct dependent",
  transitive: "Transitive dependent",
  uncertain: "Uncertain",
  unaffected: "Not affected",
  removed: "Removed",
};

/** Edge ids along the report's own impact path for `path` (empty if it has none). */
export function impactPathEdgeIds(model: ReportModel, path: string): string[] {
  const a = model.nodes.get(path)?.affected;
  if (!a) return [];
  return a.hops.map((h) => edgeId(h.importer, h.imported));
}

/** Paths on the impact path of `path`, including the changed file it starts from. */
export function impactPathNodes(model: ReportModel, path: string): string[] {
  return model.nodes.get(path)?.affected?.path_from_changed ?? [];
}
