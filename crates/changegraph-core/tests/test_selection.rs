//! Test recommendation on real fixture graphs, compared with hand-labeled scenarios in
//! `fixtures/expected/*.tests.json`.

use std::path::{Path, PathBuf};

use changegraph_core::dependency_graph::{build_graph, DependencyGraph};
use changegraph_core::discovery::{discover_sources, ExcludeSet};
use changegraph_core::git_diff::{ChangeStatus, ChangedFile};
use changegraph_core::impact::analyze_impact;
use changegraph_core::report::WarningCode;
use changegraph_core::test_mapping::{
    load_coverage_map, recommend_tests, CoverageMap, Method, TestInput, TestReport,
};
use changegraph_core::tsconfig;
use serde_json::{json, Value};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn graph_of(name: &str) -> (DependencyGraph, Option<tsconfig::TsConfig>) {
    let root = fixtures().join(name);
    let (files, _) = discover_sources(&root, &ExcludeSet::new(&[], true));
    let (cfg, _) = tsconfig::load(&root);
    let (graph, _) = build_graph(&root, &files, cfg.as_ref());
    (graph, cfg)
}

fn recommend(
    graph: &DependencyGraph,
    cfg: Option<&tsconfig::TsConfig>,
    changed: &[String],
    coverage: Option<&CoverageMap>,
) -> (TestReport, Vec<changegraph_core::report::Warning>) {
    let files: Vec<ChangedFile> = changed
        .iter()
        .map(|p| ChangedFile::new(p.clone(), None, ChangeStatus::Modified))
        .collect();
    let (impact, _) = analyze_impact(graph, &files, cfg, None);
    recommend_tests(&TestInput {
        graph,
        impact: &impact,
        coverage,
        graph_target_mismatch: false,
    })
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn check_fixture(name: &str, file: &str) {
    let (graph, cfg) = graph_of(name);
    let text = std::fs::read_to_string(fixtures().join("expected").join(file)).unwrap();
    let expected: Value = serde_json::from_str(&text).unwrap();

    for sc in expected["scenarios"].as_array().unwrap() {
        let label = sc["name"].as_str().unwrap();
        let (report, _) = recommend(&graph, cfg.as_ref(), &strings(&sc["changed"]), None);

        if let Some(exact) = sc.get("recommendations") {
            let actual: Vec<Value> = report
                .recommendations
                .iter()
                .map(|r| json!({ "path": r.path, "basis": r.basis, "methods": r.methods, "min_distance": r.min_distance }))
                .collect();
            assert_eq!(
                &Value::Array(actual),
                exact,
                "scenario `{label}`: recommendations (order matters)"
            );
        }
        let rec_paths: Vec<&str> = report
            .recommendations
            .iter()
            .map(|r| r.path.as_str())
            .collect();
        for p in strings(sc.get("recommendations_include").unwrap_or(&Value::Null)) {
            assert!(
                rec_paths.contains(&p.as_str()),
                "scenario `{label}`: missing {p}"
            );
        }
        if let Some(u) = sc.get("unmapped") {
            let actual: Vec<&str> = report
                .unmapped_modules
                .iter()
                .map(|m| m.path.as_str())
                .collect();
            assert_eq!(strings(u), actual, "scenario `{label}`: unmapped modules");
        }
        if let Some(c) = sc.get("complete") {
            assert_eq!(
                c.as_bool().unwrap(),
                report.completeness.complete,
                "scenario `{label}`: complete"
            );
        }
        if let Some(k) = sc.get("incomplete_kinds") {
            let actual: Vec<String> = report
                .completeness
                .reasons
                .iter()
                .map(|r| {
                    serde_json::to_value(r.kind)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_string()
                })
                .collect();
            assert_eq!(strings(k), actual, "scenario `{label}`: incomplete kinds");
        }

        // Invariants for every scenario: each recommendation is explained, and a heuristic-only
        // recommendation never claims a known method.
        for r in &report.recommendations {
            assert!(
                !r.explanation.is_empty() && !r.methods.is_empty(),
                "`{label}`: {} unexplained",
                r.path
            );
            if r.basis == changegraph_core::test_mapping::Basis::Heuristic {
                assert_eq!(r.methods, vec![Method::FilenameHeuristic]);
            }
        }
    }
}

#[test]
fn simple_project_scenarios_match_labels() {
    check_fixture("simple-ts-project", "simple-ts-project.tests.json");
}

#[test]
fn edge_case_project_scenarios_match_labels() {
    check_fixture("edge-cases-project", "edge-cases-project.tests.json");
}

#[test]
fn committed_coverage_map_fixture_loads_and_unknown_tests_are_reported() {
    let (graph, cfg) = graph_of("simple-ts-project");
    let map = load_coverage_map(&fixtures().join("expected/simple-ts-project.coverage-map.json"))
        .unwrap();
    let (report, warnings) = recommend(
        &graph,
        cfg.as_ref(),
        &["src/auth/session.ts".to_string()],
        Some(&map),
    );

    // Graph-confirmed tests gain the coverage_map method; nothing is lost.
    let session = report
        .recommendations
        .iter()
        .find(|r| r.path == "tests/session.test.ts")
        .unwrap();
    assert!(session.methods.contains(&Method::DirectImport));
    assert!(session.methods.contains(&Method::CoverageMap));
    assert_eq!(report.coverage_map.as_ref().unwrap().tests, 3);
    assert!(
        warnings
            .iter()
            .any(|w| w.code == WarningCode::CoverageMapIssue
                && w.message.contains("not-in-this-repo"))
    );
}

#[test]
fn recommendations_are_deterministic() {
    let (graph, cfg) = graph_of("edge-cases-project");
    let changed = vec!["src/lib/util.ts".to_string(), "src/cycle/a.ts".to_string()];
    let (a, _) = recommend(&graph, cfg.as_ref(), &changed, None);
    let (b, _) = recommend(&graph, cfg.as_ref(), &changed, None);
    assert_eq!(
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
}
