// Runtime validation of a loaded report. The UI never renders data that fails this check,
// so it cannot display relationships the engine did not report.

import { REPORT_SCHEMA } from "./report-schema.ts";
import { SUPPORTED_SCHEMA_VERSION } from "./report-types.ts";
import type { Report } from "./report-types.ts";

type Json = unknown;
type Obj = Record<string, Json>;

const isObj = (v: Json): v is Obj => typeof v === "object" && v !== null && !Array.isArray(v);

function resolveRef(root: Json, schema: Json): Json {
  if (isObj(schema) && typeof schema["$ref"] === "string") {
    let cur: Json = root;
    for (const seg of schema["$ref"].replace(/^#\//, "").split("/")) {
      cur = isObj(cur) ? cur[seg] : undefined;
    }
    return resolveRef(root, cur);
  }
  return schema;
}

function typeOk(t: string, v: Json): boolean {
  switch (t) {
    case "string":
      return typeof v === "string";
    case "integer":
      return typeof v === "number" && Number.isInteger(v);
    case "number":
      return typeof v === "number";
    case "boolean":
      return typeof v === "boolean";
    case "array":
      return Array.isArray(v);
    case "object":
      return isObj(v);
    case "null":
      return v === null;
    default:
      return false;
  }
}

/** Minimal JSON Schema subset: type, enum, const, required, properties,
 *  additionalProperties:false, items, minimum, minItems, $ref. Enough for the report schema. */
export function validateAgainstSchema(
  root: Json,
  rawSchema: Json,
  value: Json,
  at: string,
  errors: string[],
  limit = 50,
): void {
  if (errors.length >= limit) return;
  const schema = resolveRef(root, rawSchema);
  if (!isObj(schema)) return;
  const push = (m: string) => {
    if (errors.length < limit) errors.push(m);
  };

  if ("const" in schema && JSON.stringify(schema["const"]) !== JSON.stringify(value)) {
    push(`${at}: expected ${JSON.stringify(schema["const"])}, got ${JSON.stringify(value)}`);
  }
  const options = schema["enum"];
  if (Array.isArray(options) && !options.some((o) => JSON.stringify(o) === JSON.stringify(value))) {
    push(`${at}: ${JSON.stringify(value)} is not one of ${JSON.stringify(options)}`);
  }
  const t = schema["type"];
  if (typeof t === "string" && !typeOk(t, value)) {
    push(`${at}: expected ${t}`);
  } else if (Array.isArray(t) && !t.some((x) => typeof x === "string" && typeOk(x, value))) {
    push(`${at}: expected one of ${JSON.stringify(t)}`);
  }
  const min = schema["minimum"];
  if (typeof min === "number" && typeof value === "number" && value < min) {
    push(`${at}: ${value} is below the minimum ${min}`);
  }

  if (isObj(value)) {
    const required = schema["required"];
    if (Array.isArray(required)) {
      for (const key of required) {
        if (typeof key === "string" && !(key in value)) push(`${at}: missing required property "${key}"`);
      }
    }
    const props = isObj(schema["properties"]) ? schema["properties"] : {};
    for (const [key, sub] of Object.entries(value)) {
      if (key in props) {
        validateAgainstSchema(root, props[key], sub, `${at}.${key}`, errors, limit);
      } else if (schema["additionalProperties"] === false) {
        push(`${at}: unexpected property "${key}"`);
      }
    }
  }
  if (Array.isArray(value)) {
    const minItems = schema["minItems"];
    if (typeof minItems === "number" && value.length < minItems) push(`${at}: needs at least ${minItems} item(s)`);
    if (schema["items"] !== undefined) {
      for (let i = 0; i < value.length; i++) {
        validateAgainstSchema(root, schema["items"], value[i], `${at}[${i}]`, errors, limit);
      }
    }
  }
}

/** Cross-reference checks a schema cannot express. A report that fails these is internally
 *  inconsistent, and drawing it would mean inventing or hiding relationships. */
export function checkReferentialIntegrity(report: Report): string[] {
  const errors: string[] = [];
  const nodes = new Set(report.graph.nodes.map((n) => n.path));
  if (nodes.size !== report.graph.nodes.length) errors.push("graph.nodes contains duplicate paths");
  for (const e of report.graph.edges) {
    if (!nodes.has(e.from) || !nodes.has(e.to)) {
      errors.push(`graph edge ${e.from} -> ${e.to} references a file that is not in graph.nodes`);
    }
  }
  for (const list of [report.graph.external_dependencies, report.graph.unresolved_imports]) {
    for (const x of list) {
      if (!nodes.has(x.from)) errors.push(`import record from ${x.from} references a file that is not in graph.nodes`);
    }
  }
  for (const a of report.impact.affected) {
    if (!nodes.has(a.path)) errors.push(`impact.affected path ${a.path} is not in graph.nodes`);
    const last = a.path_from_changed[a.path_from_changed.length - 1];
    if (last !== a.path) errors.push(`impact path for ${a.path} does not end at the affected file`);
  }
  for (const u of report.impact.uncertain) {
    if (!nodes.has(u.path)) errors.push(`impact.uncertain path ${u.path} is not in graph.nodes`);
  }
  for (const c of report.impact.changed) {
    if (c.kind === "present" && !nodes.has(c.path)) errors.push(`changed file ${c.path} is not in graph.nodes`);
  }
  for (const r of report.test_recommendations.recommendations) {
    if (!nodes.has(r.path)) errors.push(`recommended test ${r.path} is not in graph.nodes`);
  }
  return errors.slice(0, 20);
}

export type ParseResult = { ok: true; report: Report } | { ok: false; errors: string[] };

export function parseReport(text: string): ParseResult {
  let data: Json;
  try {
    data = JSON.parse(text);
  } catch (e) {
    return { ok: false, errors: [`The file is not valid JSON (${e instanceof Error ? e.message : String(e)}).`] };
  }
  if (!isObj(data)) {
    return { ok: false, errors: ["The file is JSON, but not a ChangeGraph report (expected an object)."] };
  }

  const version = data["schema_version"];
  if (typeof version !== "string") {
    return { ok: false, errors: ['This is not a ChangeGraph report: "schema_version" is missing.'] };
  }
  if (version !== SUPPORTED_SCHEMA_VERSION) {
    return {
      ok: false,
      errors: [
        `This report uses schema ${version}, but this viewer supports ${SUPPORTED_SCHEMA_VERSION}. ` +
          "Regenerate the report with a matching `changegraph` version, or update the viewer.",
      ],
    };
  }

  const errors: string[] = [];
  validateAgainstSchema(REPORT_SCHEMA, REPORT_SCHEMA, data, "report", errors);
  if (errors.length > 0) return { ok: false, errors };

  const report = data as unknown as Report;
  const integrity = checkReferentialIntegrity(report);
  if (integrity.length > 0) return { ok: false, errors: integrity };
  return { ok: true, report };
}
