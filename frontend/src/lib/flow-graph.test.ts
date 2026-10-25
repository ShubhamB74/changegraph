import test from "node:test";
import assert from "node:assert/strict";
import { buildModel, edgeId } from "./model.ts";
import { defaultFilters, matchPaths, visibleEdges, visiblePaths } from "./filters.ts";
import { layeredLayout } from "./layout.ts";
import { edgeSpecs, nodeSpecs, EDGE_COLOR, EDGE_COLOR_ON_PATH, EDGE_COLOR_REMOVED } from "./flow-graph.ts";
import type { CanvasInput } from "./flow-graph.ts";
import { sampleReport } from "./test-utils.ts";

const S = "src/auth/session.ts";
const P = "src/api/profile.ts";
const PT = "tests/profile.test.ts";

function input(over: Partial<CanvasInput> = {}, query = ""): CanvasInput {
  const model = buildModel(sampleReport());
  const filters = { ...defaultFilters(model), query };
  const visible = visiblePaths(model, filters);
  const edges = visibleEdges(model, new Set(visible));
  return {
    model,
    visible,
    edges,
    layout: layeredLayout(visible, edges),
    selected: null,
    pathNodes: new Set(),
    pathEdges: new Set(),
    matches: matchPaths(model, query),
    queryActive: query.trim() !== "",
    ...over,
  };
}

test("every visible file becomes exactly one node, positioned by the layout, with accurate labels", () => {
  const i = input();
  const nodes = nodeSpecs(i);
  assert.deepEqual(nodes.map((n) => n.id), i.visible);
  const session = nodes.find((n) => n.id === S);
  assert.equal(session?.data.state, "changed");
  assert.equal(session?.ariaLabel, `${S}, Changed`);
  assert.deepEqual(session?.position, i.layout.positions.get(S));
  const pt = nodes.find((n) => n.id === PT);
  assert.equal(pt?.data.distance, 2);
  assert.equal(pt?.data.recommendedRank, 2);
  assert.equal(pt?.data.basis, "known");
  assert.equal(nodes.find((n) => n.id === P)?.data.recommendedRank, null);
});

test("with nothing selected, nothing is dimmed", () => {
  assert.ok(nodeSpecs(input()).every((n) => !n.data.dimmed && !n.data.matched && !n.data.selected));
  assert.ok(edgeSpecs(input()).every((e) => e.opacity === 1 && e.stroke === EDGE_COLOR));
});

test("highlighting an impact path emphasises its edges and dims everything off the path", () => {
  const base = input();
  const i = input({
    selected: PT,
    pathNodes: new Set([S, P, PT]),
    pathEdges: new Set([edgeId(P, S), edgeId(PT, P)]),
  });
  const nodes = nodeSpecs(i);
  assert.equal(nodes.find((n) => n.id === PT)?.data.selected, true);
  assert.equal(nodes.find((n) => n.id === P)?.data.dimmed, false);
  assert.equal(nodes.find((n) => n.id === "src/index.ts")?.data.dimmed, true);
  const edges = edgeSpecs(i);
  const onPath = edges.find((e) => e.id === edgeId(P, S));
  assert.equal(onPath?.stroke, EDGE_COLOR_ON_PATH);
  assert.equal(onPath?.opacity, 1);
  assert.equal(edges.find((e) => e.id === edgeId("src/index.ts", S))?.opacity, 0.2);
  assert.equal(edges.length, base.edges.length);
});

test("search dims non-matches and marks matches", () => {
  const i = input({}, "profile");
  const nodes = nodeSpecs(i);
  assert.equal(nodes.find((n) => n.id === P)?.data.matched, true);
  assert.equal(nodes.find((n) => n.id === P)?.data.dimmed, false);
  assert.equal(nodes.find((n) => n.id === S)?.data.dimmed, true);
});

test("edges point from importer to imported; removed-file and type-only edges are visibly different", () => {
  const r = sampleReport();
  r.impact.changed.push({ path: "src/gone.ts", status: "deleted", category: "source", kind: "deleted", direct_dependents: 1, transitive_dependents: 0 });
  r.impact.affected[0]!.hops = [{ importer: P, imported: "src/gone.ts", kind: "import", type_only: false, specifier: "./gone", line: 3 }];
  r.graph.edges.push({ from: PT, to: S, kind: "import", type_only: true, specifier: "../src/auth/session", line: 2 });
  const model = buildModel(r);
  const visible = visiblePaths(model, defaultFilters(model));
  const edges = visibleEdges(model, new Set(visible));
  const specs = edgeSpecs({ edges, pathEdges: new Set() });

  const ghost = specs.find((e) => e.id === edgeId(P, "src/gone.ts"));
  assert.equal(ghost?.source, P);
  assert.equal(ghost?.target, "src/gone.ts");
  assert.equal(ghost?.stroke, EDGE_COLOR_REMOVED);
  assert.ok(ghost?.dash);
  assert.equal(specs.find((e) => e.id === edgeId(PT, S))?.dash, "2 4");
  assert.equal(specs.find((e) => e.id === edgeId(P, S))?.dash, undefined);
  assert.equal(specs.find((e) => e.id === edgeId(P, S))?.ariaLabel, `${P} imports ${S}`);
});

test("the canvas never draws an edge the report does not contain", () => {
  const i = input();
  const reportPairs = new Set(i.model.report.graph.edges.map((e) => edgeId(e.from, e.to)));
  for (const e of edgeSpecs(i)) assert.ok(reportPairs.has(e.id), `${e.id} is not in the report`);
});
