//! Builds real graphs from the committed fixture projects and compares them with the
//! hand-labeled expectations in `fixtures/expected/`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use changegraph_core::dependency_graph::{build_graph, GraphReport};
use changegraph_core::discovery::{discover_sources, ExcludeSet};
use changegraph_core::report::{Warning, WarningCode};
use changegraph_core::tsconfig;
use serde_json::{json, Value};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn analyze_fixture(name: &str) -> (GraphReport, Vec<Warning>) {
    let root = fixtures().join(name);
    let (files, walk_warnings) = discover_sources(&root, &ExcludeSet::new(&[], true));
    assert!(walk_warnings.is_empty(), "{walk_warnings:?}");
    let (cfg, mut warnings) = tsconfig::load(&root);
    let (graph, graph_warnings) = build_graph(&root, &files, cfg.as_ref());
    warnings.extend(graph_warnings);
    (graph.into_report(), warnings)
}

fn sorted(mut v: Vec<Value>) -> Vec<Value> {
    v.sort_by_key(|x| x.to_string());
    v
}

/// Project a report into the shape used by the expectation files, ignoring `ignore_from`.
fn project(report: &GraphReport, ignore_from: &[&str]) -> Value {
    let keep = |from: &str| !ignore_from.contains(&from);
    let edges = report.edges.iter().filter(|e| keep(&e.from)).map(|e| {
        json!({ "from": e.from, "to": e.to, "kind": e.kind, "type_only": e.type_only })
    });
    let external = report
        .external_dependencies
        .iter()
        .filter(|e| keep(&e.from))
        .map(|e| json!({ "from": e.from, "package": e.package }));
    let unresolved = report.unresolved_imports.iter().filter(|u| keep(&u.from)).map(|u| {
        json!({ "from": u.from, "specifier": u.specifier, "reason": u.reason })
    });
    json!({
        "edges": sorted(edges.collect()),
        "external": sorted(external.collect()),
        "unresolved": sorted(unresolved.collect()),
        "cycles": report.cycles,
    })
}

fn expected(name: &str) -> Value {
    let text = std::fs::read_to_string(fixtures().join("expected").join(name)).unwrap();
    let mut v: Value = serde_json::from_str(&text).unwrap();
    for key in ["edges", "external", "unresolved"] {
        let list = v[key].as_array().cloned().unwrap_or_default();
        v[key] = Value::Array(sorted(list));
    }
    v
}

fn assert_same(actual: &Value, expected: &Value) {
    for key in ["edges", "external", "unresolved", "cycles"] {
        assert_eq!(
            actual[key],
            expected[key],
            "mismatch in `{key}`\nactual:   {}\nexpected: {}",
            serde_json::to_string_pretty(&actual[key]).unwrap(),
            serde_json::to_string_pretty(&expected[key]).unwrap()
        );
    }
}

#[test]
fn simple_project_matches_labeled_graph() {
    let (report, warnings) = analyze_fixture("simple-ts-project");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_same(&project(&report, &[]), &expected("simple-ts-project.graph.json"));
    assert_eq!(report.stats.parsed_ok, report.stats.files);
}

#[test]
fn edge_case_project_matches_labeled_graph() {
    let (report, _warnings) = analyze_fixture("edge-cases-project");
    assert_same(
        &project(&report, &["src/broken.ts"]),
        &expected("edge-cases-project.graph.json"),
    );
}

#[test]
fn parse_failures_are_surfaced_as_warnings_and_statuses() {
    let (report, warnings) = analyze_fixture("edge-cases-project");

    let statuses: BTreeMap<_, _> = report
        .nodes
        .iter()
        .map(|n| (n.path.as_str(), serde_json::to_value(n.parse_status).unwrap()))
        .collect();
    assert_eq!(statuses["src/broken.ts"], json!("partial"));
    assert_eq!(statuses["src/main.ts"], json!("ok"));

    assert!(warnings
        .iter()
        .any(|w| w.code == WarningCode::ParseError && w.message.contains("src/broken.ts")));
    assert_eq!(report.stats.parsed_partial, 1);
}

#[test]
fn external_and_unresolved_imports_never_become_edges() {
    let (report, _) = analyze_fixture("edge-cases-project");
    for e in &report.edges {
        assert!(report.nodes.iter().any(|n| n.path == e.to), "edge to non-node {}", e.to);
    }
    assert!(report.edges.iter().all(|e| e.specifier != "react" && e.specifier != "./missing"));
}

#[test]
fn graph_is_deterministic_across_runs() {
    let (a, _) = analyze_fixture("edge-cases-project");
    let (b, _) = analyze_fixture("edge-cases-project");
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
}
