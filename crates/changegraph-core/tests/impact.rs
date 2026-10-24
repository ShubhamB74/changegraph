//! Impact analysis on real graphs built from the committed fixtures, compared with
//! hand-labeled scenarios in `fixtures/expected/*.impact.json`.

use std::path::{Path, PathBuf};

use changegraph_core::dependency_graph::{build_graph, DependencyGraph};
use changegraph_core::discovery::{discover_sources, ExcludeSet};
use changegraph_core::git_diff::{ChangeStatus, ChangedFile};
use changegraph_core::impact::{analyze_impact, ImpactReport};
use changegraph_core::tsconfig;
use serde_json::Value;

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

fn impact_for(graph: &DependencyGraph, cfg: Option<&tsconfig::TsConfig>, changed: &[String]) -> ImpactReport {
    let files: Vec<ChangedFile> = changed
        .iter()
        .map(|p| ChangedFile::new(p.clone(), None, ChangeStatus::Modified))
        .collect();
    analyze_impact(graph, &files, cfg, None).0
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

fn check_fixture(name: &str, expectation_file: &str) {
    let (graph, cfg) = graph_of(name);
    let text = std::fs::read_to_string(fixtures().join("expected").join(expectation_file)).unwrap();
    let expected: Value = serde_json::from_str(&text).unwrap();

    for scenario in expected["scenarios"].as_array().unwrap() {
        let label = scenario["name"].as_str().unwrap();
        let report = impact_for(&graph, cfg.as_ref(), &strings(&scenario["changed"]));
        let affected_paths: Vec<&str> = report.affected.iter().map(|a| a.path.as_str()).collect();
        let uncertain_paths: Vec<&str> = report.uncertain.iter().map(|u| u.path.as_str()).collect();

        if let Some(exact) = scenario.get("affected") {
            let actual: Vec<Value> = report
                .affected
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "path": a.path,
                        "classification": a.classification,
                        "distance": a.distance,
                        "changed_dependency_count": a.changed_dependency_count,
                    })
                })
                .collect();
            assert_eq!(&Value::Array(actual), exact, "scenario `{label}`: affected list (order matters)");
        }
        for p in strings(scenario.get("affected_includes").unwrap_or(&Value::Null)) {
            assert!(affected_paths.contains(&p.as_str()), "scenario `{label}`: missing {p}");
        }
        if let Some(u) = scenario.get("uncertain") {
            assert_eq!(strings(u), uncertain_paths, "scenario `{label}`: uncertain list");
        }
        for p in strings(scenario.get("uncertain_includes_or_affected").unwrap_or(&Value::Null)) {
            assert!(
                affected_paths.contains(&p.as_str()) || uncertain_paths.contains(&p.as_str()),
                "scenario `{label}`: {p} must be affected or flagged uncertain, never silently ignored"
            );
        }

        // Invariants for every scenario.
        for a in &report.affected {
            assert!(!strings(&scenario["changed"]).contains(&a.path), "`{label}`: changed file listed as affected");
            assert_eq!(a.path_from_changed.last(), Some(&a.path));
            assert_eq!(a.hops.len(), a.distance, "`{label}`: one hop per step for {}", a.path);
            assert!(!a.explanation.is_empty());
        }
    }
}

#[test]
fn simple_project_scenarios_match_labels() {
    check_fixture("simple-ts-project", "simple-ts-project.impact.json");
}

#[test]
fn edge_case_project_scenarios_match_labels() {
    check_fixture("edge-cases-project", "edge-cases-project.impact.json");
}

#[test]
fn impact_is_deterministic() {
    let (graph, cfg) = graph_of("edge-cases-project");
    let changed = vec!["src/lib/util.ts".to_string(), "src/cycle/a.ts".to_string()];
    let a = impact_for(&graph, cfg.as_ref(), &changed);
    let b = impact_for(&graph, cfg.as_ref(), &changed);
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
}
