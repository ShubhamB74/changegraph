//! Guards against drift between the emitted report and the documented JSON Schema.
//!
//! A small structural validator (type, enum, const, required, properties,
//! additionalProperties=false, items, minimum, minItems, $ref) is enough for this schema and
//! avoids an extra dependency. It checks real reports produced from real Git repositories.

mod common;

use std::path::Path;

use changegraph_core::report::SCHEMA_VERSION;
use changegraph_core::{analyze, AnalysisConfig};
use common::TestRepo;
use serde_json::Value;

fn resolve<'a>(root: &'a Value, schema: &'a Value) -> &'a Value {
    match schema.get("$ref").and_then(Value::as_str) {
        Some(r) => {
            let mut cur = root;
            for seg in r.trim_start_matches("#/").split('/') {
                cur = &cur[seg];
            }
            resolve(root, cur)
        }
        None => schema,
    }
}

fn type_ok(t: &str, v: &Value) -> bool {
    match t {
        "string" => v.is_string(),
        "integer" => v.is_i64() || v.is_u64(),
        "number" => v.is_number(),
        "boolean" => v.is_boolean(),
        "array" => v.is_array(),
        "object" => v.is_object(),
        "null" => v.is_null(),
        _ => false,
    }
}

fn validate(root: &Value, schema: &Value, v: &Value, at: &str, errors: &mut Vec<String>) {
    let schema = resolve(root, schema);
    if let Some(c) = schema.get("const") {
        if c != v {
            errors.push(format!("{at}: expected const {c}, got {v}"));
        }
    }
    if let Some(Value::Array(options)) = schema.get("enum") {
        if !options.contains(v) {
            errors.push(format!("{at}: {v} is not one of {options:?}"));
        }
    }
    match schema.get("type") {
        Some(Value::String(t)) if !type_ok(t, v) => {
            errors.push(format!("{at}: expected {t}, got {v}"))
        }
        Some(Value::Array(ts)) if !ts.iter().filter_map(Value::as_str).any(|t| type_ok(t, v)) => {
            errors.push(format!("{at}: expected one of {ts:?}, got {v}"))
        }
        _ => {}
    }
    if let (Some(min), Some(n)) = (schema.get("minimum").and_then(Value::as_i64), v.as_i64()) {
        if n < min {
            errors.push(format!("{at}: {n} < minimum {min}"));
        }
    }
    if let Value::Object(map) = v {
        if let Some(Value::Array(req)) = schema.get("required") {
            for key in req.iter().filter_map(Value::as_str) {
                if !map.contains_key(key) {
                    errors.push(format!("{at}: missing required property `{key}`"));
                }
            }
        }
        let props = schema.get("properties").and_then(Value::as_object);
        for (key, value) in map {
            match props.and_then(|p| p.get(key)) {
                Some(sub) => validate(root, sub, value, &format!("{at}.{key}"), errors),
                None if schema.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    errors.push(format!("{at}: unexpected property `{key}` (not in schema)"))
                }
                None => {}
            }
        }
    }
    if let Value::Array(items) = v {
        if let Some(min) = schema.get("minItems").and_then(Value::as_u64) {
            if (items.len() as u64) < min {
                errors.push(format!("{at}: fewer than {min} items"));
            }
        }
        if let Some(item_schema) = schema.get("items") {
            for (i, item) in items.iter().enumerate() {
                validate(root, item_schema, item, &format!("{at}[{i}]"), errors);
            }
        }
    }
}

fn schema() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/schema")
        .join(format!("report-{SCHEMA_VERSION}.schema.json"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "schema file for {SCHEMA_VERSION} must exist at {}: {e}",
            path.display()
        )
    });
    serde_json::from_str(&text).unwrap()
}

fn assert_conforms(report: &changegraph_core::Report) {
    let schema = schema();
    assert_eq!(
        schema["properties"]["schema_version"]["const"],
        SCHEMA_VERSION
    );
    let value = serde_json::to_value(report).unwrap();
    let mut errors = Vec::new();
    validate(&schema, &schema, &value, "$", &mut errors);
    assert!(
        errors.is_empty(),
        "report does not conform to the schema:\n{}",
        errors.join("\n")
    );
}

/// A repository that exercises as many report fields as practical.
fn rich_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.write(
        "src/b.ts",
        "import { a } from './a';\nimport r from 'react';\nimport './missing';\nimport d from './data.json';\nexport const b = a;\n",
    );
    repo.write("src/b.test.ts", "import { b } from './b';\n");
    repo.write("src/old.ts", "export const old = 1;\n");
    repo.write(
        "src/c.ts",
        "import { old } from './old';\nexport const c = old;\n",
    );
    repo.write("src/broken.ts", "export const x = ;\n");
    repo.write("src/util.ts", "export const u = 1;\n");
    repo.write("src/util.test.ts", "test('u', () => {});\n");
    repo.write("src/cyc1.ts", "import './cyc2';\n");
    repo.write("src/cyc2.ts", "import './cyc1';\n");
    repo.write("README.md", "# hi\n");
    repo.write("package.json", "{}\n");
    repo.write(
        "tsconfig.json",
        "{ \"compilerOptions\": { \"baseUrl\": \".\" } }\n",
    );
    repo.commit_all("init");

    repo.write("src/a.ts", "export const a = 2;\n"); // modified, has dependents + a test
    repo.remove("src/old.ts"); // deleted, importer c.ts
    repo.write("src/util.ts", "export const u = 2;\n"); // heuristic-only test
    repo.write("src/cyc1.ts", "import './cyc2';\nexport const k = 1;\n");
    repo.write("README.md", "# hi there\n");
    repo.write("package.json", "{\"a\":1}\n");
    repo.write("dist/out.js", "module.exports = {};\n"); // excluded
    repo
}

#[test]
fn working_tree_report_conforms_to_schema() {
    let repo = rich_repo();
    let map_dir = tempfile::TempDir::new().unwrap();
    let map = map_dir.path().join("map.json");
    std::fs::write(
        &map,
        r#"{"version":1,"tests":{"src/b.test.ts":["src/a.ts"],"ghost.test.ts":["src/a.ts"]}}"#,
    )
    .unwrap();

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.coverage_map = Some(map);
    let report = analyze(&cfg).unwrap();

    // The scenario must actually populate the interesting sections, or the check proves little.
    assert!(!report.impact.affected.is_empty());
    assert!(!report.impact.uncertain.is_empty());
    assert!(!report.impact.ignored_changed_files.is_empty());
    assert!(report
        .impact
        .changed
        .iter()
        .any(|c| c.kind != changegraph_core::impact::ChangedKind::Present));
    assert!(!report.test_recommendations.recommendations.is_empty());
    assert!(!report.test_recommendations.unmapped_modules.is_empty());
    assert!(!report.test_recommendations.completeness.complete);
    assert!(!report.graph.cycles.is_empty());
    assert!(!report.graph.unresolved_imports.is_empty());
    assert!(!report.graph.external_dependencies.is_empty());
    assert!(report.test_recommendations.coverage_map.is_some());
    assert!(!report.warnings.is_empty());
    assert_conforms(&report);
}

#[test]
fn merge_base_report_and_untimed_report_conform_to_schema() {
    let repo = rich_repo();
    repo.commit_all("changes");

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.base = Some("HEAD~1".to_string());
    cfg.record_timing = false;
    cfg.max_depth = Some(1);
    let report = analyze(&cfg).unwrap();
    assert!(report.execution.duration_ms.is_none());
    assert!(report.comparison.merge_base_commit.is_some());
    assert_conforms(&report);
}

#[test]
fn empty_change_report_conforms_to_schema() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    let report = analyze(&AnalysisConfig::new(repo.path())).unwrap();
    assert_conforms(&report);
}

#[test]
fn the_validator_actually_rejects_bad_reports() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    let report = analyze(&AnalysisConfig::new(repo.path())).unwrap();
    let schema = schema();

    let mut value = serde_json::to_value(&report).unwrap();
    value["surprise"] = Value::Bool(true);
    value["graph"]["stats"]["files"] = Value::String("many".into());
    value["comparison"]["mode"] = Value::String("sideways".into());
    value.as_object_mut().unwrap().remove("impact");
    let mut errors = Vec::new();
    validate(&schema, &schema, &value, "$", &mut errors);
    let joined = errors.join("\n");
    assert!(
        joined.contains("unexpected property `surprise`"),
        "{joined}"
    );
    assert!(joined.contains("expected integer"), "{joined}");
    assert!(joined.contains("is not one of"), "{joined}");
    assert!(
        joined.contains("missing required property `impact`"),
        "{joined}"
    );
}
