// Deterministic layered layout, no dependencies.
//
// Edge A -> B means "A imports B". Layer 0 holds files that import nothing else in view; every
// importer sits in a higher layer than the files it imports. Left-to-right therefore reads as
// "how far a change propagates", which is the question the graph answers.
// Cycles: back edges found by DFS are ignored for layering (they are still drawn).

export interface LayoutEdge {
  from: string;
  to: string;
}

export interface Point {
  x: number;
  y: number;
}

export interface LayoutResult {
  positions: Map<string, Point>;
  layers: Map<string, number>;
  width: number;
  height: number;
}

export const NODE_WIDTH = 248;
export const NODE_HEIGHT = 54;
const COLUMN_GAP = 110;
const ROW_GAP = 16;

export function layeredLayout(paths: readonly string[], edges: readonly LayoutEdge[]): LayoutResult {
  const nodes = [...new Set(paths)].sort();
  const index = new Map(nodes.map((p, i) => [p, i]));
  const n = nodes.length;

  const deps: number[][] = Array.from({ length: n }, () => []);
  for (const e of edges) {
    const a = index.get(e.from);
    const b = index.get(e.to);
    if (a === undefined || b === undefined || a === b) continue;
    deps[a]?.push(b);
  }
  for (const list of deps) {
    list.sort((x, y) => x - y);
    for (let i = list.length - 1; i > 0; i--) if (list[i] === list[i - 1]) list.splice(i, 1);
  }

  // Iterative DFS: drop back edges and record post-order (dependencies before dependents).
  const color = new Uint8Array(n); // 0 new, 1 on stack, 2 done
  const dag: number[][] = Array.from({ length: n }, () => []);
  const post: number[] = [];
  for (let root = 0; root < n; root++) {
    if (color[root] !== 0) continue;
    const stack: [number, number][] = [[root, 0]];
    color[root] = 1;
    while (stack.length > 0) {
      const top = stack[stack.length - 1];
      if (!top) break;
      const [v, i] = top;
      const children = deps[v] ?? [];
      if (i < children.length) {
        top[1] = i + 1;
        const w = children[i] as number;
        if (color[w] === 1) continue; // back edge: ignore for layering
        dag[v]?.push(w);
        if (color[w] === 0) {
          color[w] = 1;
          stack.push([w, 0]);
        }
      } else {
        color[v] = 2;
        post.push(v);
        stack.pop();
      }
    }
  }

  const layer = new Int32Array(n);
  for (const v of post) {
    let l = 0;
    for (const w of dag[v] ?? []) l = Math.max(l, (layer[w] ?? 0) + 1);
    layer[v] = l;
  }

  const maxLayer = n === 0 ? 0 : Math.max(...layer);
  const columns: number[][] = Array.from({ length: maxLayer + 1 }, () => []);
  for (let v = 0; v < n; v++) columns[layer[v] as number]?.push(v);

  // Barycenter sweeps to reduce crossings; ties broken by path order, so results are stable.
  const dependents: number[][] = Array.from({ length: n }, () => []);
  for (let v = 0; v < n; v++) for (const w of deps[v] ?? []) dependents[w]?.push(v);
  const pos = new Float64Array(n);
  const refresh = () => {
    for (const col of columns) col.forEach((v, i) => (pos[v] = col.length > 1 ? i / (col.length - 1) : 0.5));
  };
  refresh();
  const sweep = (order: number[], neighbours: number[][]) => {
    for (const l of order) {
      const col = columns[l];
      if (!col) continue;
      const key = new Map<number, number>();
      for (const v of col) {
        const ns = neighbours[v] ?? [];
        key.set(v, ns.length === 0 ? pos[v] ?? 0.5 : ns.reduce((s, w) => s + (pos[w] ?? 0.5), 0) / ns.length);
      }
      col.sort((a, b) => (key.get(a) as number) - (key.get(b) as number) || a - b);
      col.forEach((v, i) => (pos[v] = col.length > 1 ? i / (col.length - 1) : 0.5));
    }
  };
  const ascending = Array.from({ length: maxLayer + 1 }, (_, i) => i);
  const descending = [...ascending].reverse();
  for (let round = 0; round < 4; round++) {
    sweep(ascending, deps);
    sweep(descending, dependents);
  }

  const tallest = Math.max(0, ...columns.map((c) => c.length));
  const colHeight = (len: number) => (len === 0 ? 0 : len * NODE_HEIGHT + (len - 1) * ROW_GAP);
  const height = colHeight(tallest);
  const positions = new Map<string, Point>();
  const layers = new Map<string, number>();
  columns.forEach((col, l) => {
    const offset = (height - colHeight(col.length)) / 2;
    col.forEach((v, row) => {
      const path = nodes[v] as string;
      positions.set(path, { x: l * (NODE_WIDTH + COLUMN_GAP), y: offset + row * (NODE_HEIGHT + ROW_GAP) });
      layers.set(path, l);
    });
  });
  return {
    positions,
    layers,
    width: n === 0 ? 0 : (maxLayer + 1) * NODE_WIDTH + maxLayer * COLUMN_GAP,
    height,
  };
}
