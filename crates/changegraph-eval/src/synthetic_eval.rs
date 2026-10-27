//! Scores extraction, impact and test selection against the exact truth of a synthetic repo.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use changegraph_core::test_mapping::Basis;
use changegraph_core::{analyze, AnalysisConfig, Report};
use serde::Serialize;

use crate::gitutil::{commit_all, git, init_repo};
use crate::metrics::{compare, reduction, Score};
use crate::synthetic::{generate, truth_impact, SyntheticSpec, Truth};

#[derive(Debug, Clone, Serialize)]
pub struct TargetResult {
    pub target: String,
    pub truth_affected: usize,
    pub impact: Score,
    /// Of the correctly found affected files, how many have exactly the true shortest distance.
    pub distance_exact: usize,
    pub tests_known: Score,
    pub recommended: usize,
    pub total_test_files: usize,
    pub reduction: Option<f64>,
    pub engine_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyntheticResult {
    pub files: usize,
    pub seed: u64,
    pub modules: usize,
    pub tests: usize,
    pub true_edges: usize,
    pub extraction_edges: Score,
    pub extraction_external: Score,
    pub extraction_unresolved: Score,
    pub parse_clean_share: Option<f64>,
    pub targets: Vec<TargetResult>,
}

fn extraction(report: &Report, truth: &Truth) -> (Score, Score, Score) {
    let edges: BTreeSet<(String, String)> = report
        .graph
        .edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();
    let external: BTreeSet<(String, String)> = report
        .graph
        .external_dependencies
        .iter()
        .map(|e| (e.from.clone(), e.package.clone()))
        .collect();
    let unresolved: BTreeSet<(String, String)> = report
        .graph
        .unresolved_imports
        .iter()
        .map(|u| (u.from.clone(), u.specifier.clone()))
        .collect();
    (
        compare(&edges, &truth.edges).into(),
        compare(&external, &truth.external).into(),
        compare(&unresolved, &truth.unresolved).into(),
    )
}

/// The hub (module 0, imported by many), a middle module, and the last module.
pub fn pick_targets(modules: &[String]) -> Vec<String> {
    let mut idx = vec![0, modules.len() / 2, modules.len().saturating_sub(1)];
    idx.dedup();
    idx.into_iter().map(|i| modules[i].clone()).collect()
}

pub fn evaluate(spec: &SyntheticSpec) -> Result<SyntheticResult> {
    let tmp = tempfile::TempDir::new()?;
    let dir = tmp.path();
    let truth = generate(dir, spec)?;
    init_repo(dir)?;
    commit_all(dir, "synthetic")?;

    let mut extraction_scores: Option<(Score, Score, Score)> = None;
    let mut parse_clean = None;
    let mut targets = Vec::new();

    for target in pick_targets(&truth.modules) {
        let mut text = std::fs::read_to_string(dir.join(&target))?;
        text.push_str("// edit\n");
        std::fs::write(dir.join(&target), text)?;

        let report = analyze(&AnalysisConfig::new(dir))?;
        if extraction_scores.is_none() {
            extraction_scores = Some(extraction(&report, &truth));
            let g = &report.graph.stats;
            parse_clean = (g.files > 0).then(|| g.parsed_ok as f64 / g.files as f64);
        }

        let truth_map = truth_impact(&truth.edges, std::slice::from_ref(&target));
        let tool_map: BTreeMap<String, usize> = report
            .impact
            .affected
            .iter()
            .map(|a| (a.path.clone(), a.distance))
            .collect();
        let tool_set: BTreeSet<String> = tool_map.keys().cloned().collect();
        let truth_set: BTreeSet<String> = truth_map.keys().cloned().collect();

        let truth_tests: BTreeSet<String> = truth_set
            .iter()
            .filter(|p| truth.tests.contains(*p))
            .cloned()
            .collect();
        let recs = &report.test_recommendations.recommendations;
        let known: BTreeSet<String> = recs
            .iter()
            .filter(|r| r.basis == Basis::Known)
            .map(|r| r.path.clone())
            .collect();

        let total = report.test_recommendations.summary.total_test_files;
        targets.push(TargetResult {
            target: target.clone(),
            truth_affected: truth_set.len(),
            impact: compare(&tool_set, &truth_set).into(),
            distance_exact: truth_map
                .iter()
                .filter(|(p, d)| tool_map.get(*p) == Some(*d))
                .count(),
            tests_known: compare(&known, &truth_tests).into(),
            recommended: recs.len(),
            total_test_files: total,
            reduction: reduction(recs.len(), total),
            engine_ms: report.execution.duration_ms,
        });

        git(dir, &["checkout", "-q", "--", "."])?;
    }

    let (edges, external, unresolved) = extraction_scores.expect("at least one target");
    Ok(SyntheticResult {
        files: spec.files,
        seed: spec.seed,
        modules: truth.modules.len(),
        tests: truth.tests.len(),
        true_edges: truth.edges.len(),
        extraction_edges: edges,
        extraction_external: external,
        extraction_unresolved: unresolved,
        parse_clean_share: parse_clean,
        targets,
    })
}
