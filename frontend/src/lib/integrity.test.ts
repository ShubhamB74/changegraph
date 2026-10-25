import test from "node:test";
import assert from "node:assert/strict";
import { buildModel } from "./model.ts";
import { integrityNotes } from "./integrity.ts";
import { sampleReport } from "./test-utils.ts";

const ids = (r = sampleReport()) => integrityNotes(buildModel(r)).map((n) => n.id);

test("the sample's incomplete test selection is surfaced as a limit", () => {
  const notes = integrityNotes(buildModel(sampleReport()));
  const n = notes.find((x) => x.id === "tests-unmapped_modules");
  assert.equal(n?.level, "limit");
  assert.match(n?.text ?? "", /src\/index\.ts/);
});

test("parse failures, unresolved imports and uncertain files each raise a caution", () => {
  const r = sampleReport();
  r.graph.stats.parsed_partial = 2;
  r.graph.stats.unresolved_imports = 3;
  r.impact.uncertain = [{ path: "src/api/profile.ts", category: "source", reasons: [{ kind: "parse_partial" }] }];
  const got = ids(r);
  assert.ok(got.includes("parse") && got.includes("unresolved") && got.includes("uncertain"));
  const text = integrityNotes(buildModel(r)).find((n) => n.id === "parse")?.text ?? "";
  assert.match(text, /2 files/);
});

test("depth limit, graph/target mismatch and dirty tree are flagged", () => {
  const r = sampleReport();
  r.impact.depth_limited = true;
  r.impact.max_depth = 2;
  r.warnings.push(
    { code: "graph_target_mismatch", message: "x" },
    { code: "dirty_working_tree", message: "y" },
  );
  const got = ids(r);
  assert.ok(got.includes("depth") && got.includes("mismatch") && got.includes("dirty"));
});

test("a clean, complete report has no limits or cautions; an empty one says so", () => {
  const clean = sampleReport();
  clean.test_recommendations.completeness = { complete: true, reasons: [] };
  assert.deepEqual(ids(clean), []);

  const empty = sampleReport();
  empty.impact.summary.changed_files = 0;
  empty.impact.changed = [];
  empty.impact.affected = [];
  empty.test_recommendations.completeness = { complete: true, reasons: [] };
  assert.deepEqual(ids(empty), ["nochange"]);
});
