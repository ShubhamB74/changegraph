import test from "node:test";
import assert from "node:assert/strict";
import { failedTestFiles, newlyFailing, parsePairs } from "./collect-outcomes.mjs";

const report = {
  testResults: [
    { name: "/repo/tests/a.test.ts", status: "passed", assertionResults: [{ status: "passed" }] },
    { name: "/repo/tests/b.test.ts", status: "failed", assertionResults: [{ status: "failed" }, { status: "passed" }] },
    { name: "/repo/tests/broken-suite.test.ts", status: "failed", assertionResults: [] },
    { name: "/repo/tests/mixed.test.ts", status: "passed", assertionResults: [{ status: "failed" }] },
    { name: "C:\\repo\\tests\\win.test.ts", status: "failed" },
    { status: "failed" },
  ],
};

test("failed files are repo-relative POSIX paths; suite-level failures count; passes do not", () => {
  const got = [...failedTestFiles(report, "/repo")].sort();
  assert.deepEqual(got, [
    "C:/repo/tests/win.test.ts", // a foreign absolute path is left alone, only separators are normalised
    "tests/b.test.ts",
    "tests/broken-suite.test.ts",
    "tests/mixed.test.ts",
  ]);
});

test("garbage or empty reports yield no failures instead of throwing", () => {
  assert.equal(failedTestFiles({}, "/repo").size, 0);
  assert.equal(failedTestFiles(null, "/repo").size, 0);
  assert.equal(failedTestFiles({ testResults: "nope" }, "/repo").size, 0);
});

test("newly failing = fails in every head run and in no base run", () => {
  const s = (...x) => new Set(x);
  const head = [s("a", "b", "flaky"), s("a", "b")];
  const base = [s("b"), s("b", "other")];
  assert.deepEqual(newlyFailing(head, base), ["a"]);
  assert.deepEqual(newlyFailing([], base), []);
  assert.deepEqual(newlyFailing([s("x")], []), ["x"]);
});

test("a test failing at base is never blamed on the change, even if it also fails at head", () => {
  assert.deepEqual(newlyFailing([new Set(["t"])], [new Set(["t"])]), []);
});

test("pairs files support comments and reject malformed lines", () => {
  assert.deepEqual(parsePairs("# header\nabc123 def456  # trailing\n\n111 222\n"), [
    { base: "abc123", head: "def456" },
    { base: "111", head: "222" },
  ]);
  assert.throws(() => parsePairs("onlyone"));
  assert.throws(() => parsePairs("a b c"));
});
