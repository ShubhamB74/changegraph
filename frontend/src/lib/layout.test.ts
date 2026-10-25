import test from "node:test";
import assert from "node:assert/strict";
import { layeredLayout, NODE_HEIGHT, NODE_WIDTH } from "./layout.ts";
import { lcg } from "./test-utils.ts";

const e = (from: string, to: string) => ({ from, to });

test("importers sit to the right of what they import", () => {
  // a imports b imports c
  const r = layeredLayout(["a", "b", "c"], [e("a", "b"), e("b", "c")]);
  assert.equal(r.layers.get("c"), 0);
  assert.equal(r.layers.get("b"), 1);
  assert.equal(r.layers.get("a"), 2);
  const x = (p: string) => r.positions.get(p)?.x ?? NaN;
  assert.ok(x("c") < x("b") && x("b") < x("a"));
});

test("cycles terminate, every node is placed, and self-loops/unknown endpoints are ignored", () => {
  const r = layeredLayout(["a", "b", "c"], [e("a", "b"), e("b", "a"), e("c", "a"), e("c", "c"), e("c", "ghost")]);
  assert.equal(r.positions.size, 3);
  assert.equal(new Set([...r.positions.values()].map((p) => `${p.x},${p.y}`)).size, 3);
});

test("nodes never overlap", () => {
  const r = layeredLayout(["a", "b", "c", "d", "e"], [e("e", "a"), e("e", "b"), e("e", "c"), e("e", "d")]);
  const pts = [...r.positions.values()];
  for (let i = 0; i < pts.length; i++) {
    for (let j = i + 1; j < pts.length; j++) {
      const a = pts[i]!;
      const b = pts[j]!;
      const overlapX = Math.abs(a.x - b.x) < NODE_WIDTH;
      const overlapY = Math.abs(a.y - b.y) < NODE_HEIGHT;
      assert.ok(!(overlapX && overlapY), `${JSON.stringify(a)} overlaps ${JSON.stringify(b)}`);
    }
  }
});

test("the result does not depend on input order", () => {
  const paths = ["a", "b", "c", "d", "e", "f"];
  const edges = [e("a", "b"), e("b", "c"), e("d", "c"), e("e", "b"), e("f", "a"), e("f", "e")];
  const one = layeredLayout(paths, edges);
  const two = layeredLayout([...paths].reverse(), [...edges].reverse());
  assert.deepEqual([...one.positions].sort(), [...two.positions].sort());
});

test("empty input is fine", () => {
  const r = layeredLayout([], []);
  assert.equal(r.positions.size, 0);
  assert.equal(r.width, 0);
});

test("a very deep chain does not overflow the stack", () => {
  const n = 5000;
  const paths = Array.from({ length: n }, (_, i) => `f${String(i).padStart(5, "0")}`);
  const edges = paths.slice(1).map((p, i) => e(p, paths[i]!));
  const r = layeredLayout(paths, edges);
  assert.equal(r.layers.get(paths[n - 1]!), n - 1);
});

test("a 1500-file, 6000-import graph lays out quickly and respects every dependency", () => {
  const rnd = lcg(42);
  const n = 1500;
  const paths = Array.from({ length: n }, (_, i) => `src/m${String(i).padStart(4, "0")}.ts`);
  const edges: { from: string; to: string }[] = [];
  while (edges.length < 6000) {
    const a = Math.floor(rnd() * n);
    const b = Math.floor(rnd() * n);
    if (a > b) edges.push(e(paths[a]!, paths[b]!)); // DAG: higher index imports lower
  }
  const t0 = performance.now();
  const r = layeredLayout(paths, edges);
  const ms = performance.now() - t0;
  assert.equal(r.positions.size, n);
  for (const ed of edges) assert.ok((r.layers.get(ed.from) ?? 0) > (r.layers.get(ed.to) ?? 0));
  assert.ok(ms < 5000, `layout took ${ms.toFixed(0)} ms`);
});
