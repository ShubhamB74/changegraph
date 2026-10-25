import test from "node:test";
import assert from "node:assert/strict";
import { parseReport } from "./validate-report.ts";
import { sampleReport, sampleText } from "./test-utils.ts";
import type { Report } from "./report-types.ts";

const parse = (r: unknown) => parseReport(JSON.stringify(r));

test("the bundled sample report is accepted", () => {
  const result = parseReport(sampleText());
  assert.equal(result.ok, true, result.ok ? "" : result.errors.join("\n"));
});

test("invalid JSON and non-objects get plain-language errors", () => {
  const a = parseReport("{ nope");
  assert.equal(a.ok, false);
  assert.match(a.ok ? "" : a.errors[0] ?? "", /not valid JSON/);
  const b = parseReport("[1,2]");
  assert.equal(b.ok, false);
  assert.match(b.ok ? "" : b.errors[0] ?? "", /not a ChangeGraph report/);
});

test("a missing or unsupported schema_version is explained, not guessed at", () => {
  const missing = parse({ hello: "world" });
  assert.equal(missing.ok, false);
  assert.match(missing.ok ? "" : missing.errors[0] ?? "", /schema_version/);

  const old = sampleReport();
  old.schema_version = "0.3.0";
  const r = parse(old);
  assert.equal(r.ok, false);
  assert.match(r.ok ? "" : r.errors[0] ?? "", /supports 0\.4\.0/);
});

test("structural problems are reported with the offending path", () => {
  const noImpact: Record<string, unknown> = { ...sampleReport() };
  delete noImpact["impact"];
  const a = parse(noImpact);
  assert.equal(a.ok, false);
  assert.ok((a.ok ? [] : a.errors).some((e) => e.includes('missing required property "impact"')));

  const badEnum = sampleReport();
  (badEnum.graph.nodes[0] as { parse_status: string }).parse_status = "great";
  const b = parse(badEnum);
  assert.equal(b.ok, false);
  assert.ok((b.ok ? [] : b.errors).some((e) => e.includes("graph.nodes[0].parse_status")));

  const extra = { ...sampleReport(), surprise: true };
  const c = parse(extra);
  assert.equal(c.ok, false);
  assert.ok((c.ok ? [] : c.errors).some((e) => e.includes('unexpected property "surprise"')));

  const wrongType = sampleReport();
  (wrongType.graph.stats as unknown as { files: string }).files = "five";
  assert.equal(parse(wrongType).ok, false);
});

test("a report that contradicts itself is rejected rather than drawn", () => {
  const dangling: Report = sampleReport();
  dangling.graph.edges.push({
    from: "src/api/profile.ts",
    to: "src/does-not-exist.ts",
    kind: "import",
    type_only: false,
    specifier: "./does-not-exist",
    line: 9,
  });
  const a = parse(dangling);
  assert.equal(a.ok, false);
  assert.ok((a.ok ? [] : a.errors).some((e) => e.includes("not in graph.nodes")));

  const ghostAffected: Report = sampleReport();
  const first = ghostAffected.impact.affected[0];
  assert.ok(first);
  first.path = "src/phantom.ts";
  first.path_from_changed = ["src/auth/session.ts", "src/phantom.ts"];
  const b = parse(ghostAffected);
  assert.equal(b.ok, false);
  assert.ok((b.ok ? [] : b.errors).some((e) => e.includes("src/phantom.ts")));

  const badPath: Report = sampleReport();
  const second = badPath.impact.affected[1];
  assert.ok(second);
  second.path_from_changed = ["src/auth/session.ts", "src/api/profile.ts"];
  assert.equal(parse(badPath).ok, false);
});

test("nullable fields are accepted (no timing, no coverage map)", () => {
  const r = sampleReport();
  r.execution.duration_ms = null;
  r.test_recommendations.coverage_map = null;
  assert.equal(parse(r).ok, true);
});
