import test from "node:test";
import assert from "node:assert/strict";
import { buildModel, edgeId, impactPathEdgeIds, impactPathNodes } from "./model.ts";
import { sampleReport } from "./test-utils.ts";

const S = "src/auth/session.ts";
const P = "src/api/profile.ts";
const I = "src/index.ts";
const PT = "tests/profile.test.ts";
const ST = "tests/session.test.ts";

test("states, distances and ranks come straight from the report", () => {
  const m = buildModel(sampleReport());
  const state = (p: string) => m.nodes.get(p)?.state;
  assert.equal(state(S), "changed");
  assert.equal(state(P), "direct");
  assert.equal(state(I), "direct");
  assert.equal(state(ST), "direct");
  assert.equal(state(PT), "transitive");
  assert.equal(m.nodes.get(PT)?.distance, 2);
  assert.equal(m.nodes.get(P)?.rank, 1);
  assert.equal(m.nodes.get(S)?.changeStatus, "modified");
  assert.equal(m.impactedCount, 5);
  assert.deepEqual(m.order, [P, S, I, PT, ST].sort());
});

test("edges mirror graph.edges one-to-one and adjacency follows edge direction", () => {
  const m = buildModel(sampleReport());
  assert.equal(m.edges.length, 5);
  assert.ok(m.edges.every((e) => !e.ghost));
  assert.deepEqual(m.nodes.get(P)?.dependsOn, [S]);
  assert.deepEqual(m.nodes.get(S)?.dependedOnBy, [P, I, ST].sort());
  assert.deepEqual(m.nodes.get(S)?.dependsOn, []);
  assert.equal(m.edgesById.get(edgeId(I, S))?.statements[0]?.kind, "re_export_all");
});

test("several import statements between one pair collapse into one drawn edge, none lost", () => {
  const r = sampleReport();
  r.graph.edges.push({ from: I, to: S, kind: "import", type_only: true, specifier: "./auth/session", line: 7 });
  const m = buildModel(r);
  assert.equal(m.edges.length, 5);
  assert.equal(m.edgesById.get(edgeId(I, S))?.statements.length, 2);
});

test("tests are linked to the modules they cover, and an untested module stays untested", () => {
  const m = buildModel(sampleReport());
  assert.deepEqual(m.nodes.get(S)?.coveredBy.map((t) => t.path).sort(), [PT, ST].sort());
  assert.deepEqual(m.nodes.get(P)?.coveredBy.map((t) => t.path), [PT]);
  assert.deepEqual(m.nodes.get(I)?.coveredBy, []);
  assert.equal(m.nodes.get(PT)?.recommendation?.rank, 2);
  assert.equal(m.nodes.get(P)?.recommendation, null);
});

test("impact paths expose the report's own hops and nodes", () => {
  const m = buildModel(sampleReport());
  assert.deepEqual(impactPathNodes(m, PT), [S, P, PT]);
  assert.deepEqual(impactPathEdgeIds(m, PT), [edgeId(P, S), edgeId(PT, P)]);
  assert.deepEqual(impactPathNodes(m, S), []);
});

test("removed files appear as ghost nodes with an unresolved-import edge taken from the hops", () => {
  const r = sampleReport();
  r.impact.changed.push({
    path: "src/gone.ts",
    status: "deleted",
    category: "source",
    kind: "deleted",
    direct_dependents: 1,
    transitive_dependents: 0,
  });
  const a = r.impact.affected[0];
  assert.ok(a);
  a.hops = [{ importer: P, imported: "src/gone.ts", kind: "import", type_only: false, specifier: "./gone", line: 3 }];
  const m = buildModel(r);
  const ghost = m.nodes.get("src/gone.ts");
  assert.equal(ghost?.state, "removed");
  assert.equal(ghost?.ghost, true);
  assert.equal(ghost?.parseStatus, null);
  const e = m.edgesById.get(edgeId(P, "src/gone.ts"));
  assert.equal(e?.ghost, true);
  assert.equal(e?.statements[0]?.line, 3);
  assert.ok(m.nodes.get(P)?.dependsOn.includes("src/gone.ts"));
});

test("uncertain files keep their reasons; affected files are never downgraded to uncertain", () => {
  const r = sampleReport();
  r.graph.nodes.push({ path: "src/lonely.ts", category: "source", parse_status: "partial" });
  r.impact.uncertain = [
    { path: "src/lonely.ts", category: "source", reasons: [{ kind: "parse_partial" }] },
    { path: P, category: "source", reasons: [{ kind: "unresolved_import", specifier: "./x", line: 2 }] },
  ];
  const m = buildModel(r);
  assert.equal(m.nodes.get("src/lonely.ts")?.state, "uncertain");
  assert.equal(m.nodes.get("src/lonely.ts")?.uncertain?.reasons[0]?.kind, "parse_partial");
  assert.equal(m.nodes.get(P)?.state, "direct");
});

test("unresolved and external imports attach to the importing file", () => {
  const r = sampleReport();
  r.graph.unresolved_imports.push({
    from: P, specifier: "./missing", reason: "not_found", kind: "import", type_only: false, line: 4,
  });
  r.graph.external_dependencies.push({
    from: P, package: "react", specifier: "react", kind: "import", type_only: false, line: 2,
  });
  const m = buildModel(r);
  assert.equal(m.nodes.get(P)?.unresolved[0]?.specifier, "./missing");
  assert.equal(m.nodes.get(P)?.external[0]?.package, "react");
});
