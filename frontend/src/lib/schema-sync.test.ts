import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { REPORT_SCHEMA, REPORT_SCHEMA_FILE } from "./report-schema.ts";
import { SUPPORTED_SCHEMA_VERSION } from "./report-types.ts";

const dir = new URL("../../../docs/schema/", import.meta.url);

test("the embedded schema equals the canonical docs/schema file", () => {
  const canonical = JSON.parse(readFileSync(new URL(REPORT_SCHEMA_FILE, dir), "utf8"));
  assert.deepEqual(REPORT_SCHEMA, canonical, "run `npm run sync-schema`");
});

test("the viewer targets the newest schema the engine documents", () => {
  const files = readdirSync(dir)
    .filter((f) => /^report-\d+\.\d+\.\d+\.schema\.json$/.test(f))
    .sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  assert.equal(files[files.length - 1], REPORT_SCHEMA_FILE, "a newer schema exists; update the viewer");
  assert.equal(REPORT_SCHEMA_FILE, `report-${SUPPORTED_SCHEMA_VERSION}.schema.json`);
});
