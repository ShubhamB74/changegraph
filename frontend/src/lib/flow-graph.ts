// Pure description of what the canvas draws: which nodes/edges, how they are styled and when
// they are dimmed. Kept free of React and React Flow so it can be unit-tested.

import { splitPath } from "./format.ts";
import type { LayoutResult, Point } from "./layout.ts";
import { STATE_LABEL } from "./model.ts";
import type { ImpactState, PairEdge, ReportModel } from "./model.ts";
import type { ParseStatus } from "./report-types.ts";

export type FileNodeData = {
  base: string;
  dir: string;
  state: ImpactState;
  category: "source" | "test";
  parse: ParseStatus | null;
  distance: number | null;
  recommendedRank: number | null;
  basis: "known" | "heuristic" | null;
  selected: boolean;
  dimmed: boolean;
  matched: boolean;
};

export interface NodeSpec {
  id: string;
  position: Point;
  data: FileNodeData;
  ariaLabel: string;
}

export interface EdgeSpec {
  id: string;
  source: string;
  target: string;
  stroke: string;
  strokeWidth: number;
  dash: string | undefined;
  opacity: number;
  zIndex: number;
  ariaLabel: string;
}

export interface CanvasInput {
  model: ReportModel;
  visible: readonly string[];
  edges: readonly PairEdge[];
  layout: LayoutResult;
  selected: string | null;
  pathNodes: ReadonlySet<string>;
  pathEdges: ReadonlySet<string>;
  matches: ReadonlySet<string>;
  queryActive: boolean;
}

export const EDGE_COLOR = "#4A5970";
export const EDGE_COLOR_ON_PATH = "#F4F7FB";
export const EDGE_COLOR_REMOVED = "#F5B942";

export function nodeSpecs(i: CanvasInput): NodeSpec[] {
  const pathActive = i.pathEdges.size > 0;
  return i.visible.map((path) => {
    const n = i.model.nodes.get(path);
    const { dir, base } = splitPath(path);
    const state: ImpactState = n?.state ?? "unaffected";
    const data: FileNodeData = {
      base,
      dir,
      state,
      category: n?.category ?? "source",
      parse: n?.parseStatus ?? null,
      distance: n?.distance ?? null,
      recommendedRank: n?.recommendation?.rank ?? null,
      basis: n?.recommendation?.basis ?? null,
      selected: path === i.selected,
      dimmed: (i.queryActive && !i.matches.has(path)) || (pathActive && !i.pathNodes.has(path)),
      matched: i.queryActive && i.matches.has(path),
    };
    return {
      id: path,
      position: i.layout.positions.get(path) ?? { x: 0, y: 0 },
      data,
      ariaLabel: `${path}, ${STATE_LABEL[state]}`,
    };
  });
}

export function edgeSpecs(i: Pick<CanvasInput, "edges" | "pathEdges">): EdgeSpec[] {
  const pathActive = i.pathEdges.size > 0;
  return i.edges.map((e) => {
    const on = i.pathEdges.has(e.id);
    const typeOnly = e.statements.every((s) => s.type_only);
    return {
      id: e.id,
      // Arrow from the importing file to the file it imports.
      source: e.from,
      target: e.to,
      stroke: on ? EDGE_COLOR_ON_PATH : e.ghost ? EDGE_COLOR_REMOVED : EDGE_COLOR,
      strokeWidth: on ? 2.6 : 1.3,
      dash: e.ghost ? "6 5" : typeOnly ? "2 4" : undefined,
      opacity: pathActive && !on ? 0.2 : 1,
      zIndex: on ? 10 : 0,
      ariaLabel: `${e.from} imports ${e.to}`,
    };
  });
}
