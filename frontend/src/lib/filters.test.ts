import test from "node:test";
import assert from "node:assert/strict";
import { buildModel } from "./model.ts";
import { defaultFilters, matchPaths, visibleEdges, visiblePaths } from "./filters.ts";
import { sampleReport } from "./test-utils.ts";

function modelWithBystander() {
  const r = sampleReport();
  r.graph.nodes.push({ path: "src/bystander.ts", category: "source", parse_status: "ok" });
  return buildModel(r);
}

test("default scope shows the impact; unrelated files stay out until asked for", () => {
  const m = modelWithBystander();
  const f = defaultFilters(m);
  assert.equal(f.scope, "impact");
  assert.ok(!visiblePaths(m, f).includes("src/bystander.ts"));
  assert.ok(visiblePaths(m, { ...f, scope: "all" }).includes("src/bystander.ts"));
});

test("search can reach files outside the impact scope and matches case-insensitively", () => {
  const m = modelWithBystander();
  const f = { ...defaultFilters(m), query: "BYSTANDER" };
  assert.deepEqual([...matchPaths(m, f.query)], ["src/bystander.ts"]);
  assert.ok(visiblePaths(m, f).includes("src/bystander.ts"));
  assert.deepEqual([...matchPaths(m, "   ")], []);
});

test("state and test toggles filter, but forced paths (an expanded impact path) always stay", () => {
  const m = buildModel(sampleReport());
  const f = defaultFilters(m);
  const noDirect = { ...f, states: { ...f.states, direct: false } };
  const v = visiblePaths(m, noDirect);
  assert.ok(!v.includes("src/api/profile.ts"));
  assert.ok(v.includes("src/auth/session.ts"));
  assert.ok(visiblePaths(m, noDirect, new Set(["src/api/profile.ts"])).includes("src/api/profile.ts"));

  const noTests = visiblePaths(m, { ...f, showTests: false });
  assert.ok(noTests.every((p) => !p.startsWith("tests/")));
});

test("with nothing impacted the default is to show everything, not an empty canvas", () => {
  const r = sampleReport();
  r.impact.changed = [];
  r.impact.affected = [];
  const m = buildModel(r);
  const f = defaultFilters(m);
  assert.equal(f.scope, "all");
  assert.equal(visiblePaths(m, f).length, 5);
});

test("edges are drawn only when both ends are visible", () => {
  const m = buildModel(sampleReport());
  const all = visibleEdges(m, new Set(m.order));
  assert.equal(all.length, 5);
  const some = visibleEdges(m, new Set(["src/auth/session.ts", "src/api/profile.ts"]));
  assert.deepEqual(some.map((e) => e.id), ["src/api/profile.ts -> src/auth/session.ts"]);
});

test("filtering a large model stays fast", () => {
  const r = sampleReport();
  for (let i = 0; i < 5000; i++) r.graph.nodes.push({ path: `gen/f${String(i).padStart(5, "0")}.ts`, category: "source", parse_status: "ok" });
  const m = buildModel(r);
  const t0 = performance.now();
  const f = { ...defaultFilters(m), query: "f0420" };
  const v = visiblePaths(m, f);
  assert.ok(v.includes("gen/f04200.ts"));
  assert.ok(performance.now() - t0 < 1000);
});
