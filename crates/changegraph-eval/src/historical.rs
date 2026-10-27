//! Evaluation against recorded test outcomes of real commits.
//!
//! A dataset lists, per commit pair, the test files that **newly fail** at `head` compared with
//! `base` (collected by `eval/historical/collect-outcomes.mjs`, which actually runs the project's
//! test suite). For each pair the real analyzer is run from a checkout of `head`, and we ask:
//! of the tests that really broke, how many did the tool recommend (and how much of the suite
//! did it propose to skip)?
//!
//! Validity limits, repeated in every report: small samples, a single project's conventions,
//! flaky tests, and "newly failing" being only a proxy for "affected by the change".

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use changegraph_core::test_mapping::Basis;
use changegraph_core::{analyze, AnalysisConfig};
use serde::{Deserialize, Serialize};

use crate::gitutil::git;
use crate::metrics::{reduction, wilson_interval};

#[derive(Debug, Deserialize)]
pub struct Dataset {
    pub version: u32,
    /// Path of the repository, relative to the dataset file (or absolute).
    pub repo: String,
    #[serde(default)]
    pub description: String,
    pub cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
pub struct Case {
    pub id: String,
    pub base: String,
    pub head: String,
    /// Test files that fail at `head` but pass at `base`.
    #[serde(default)]
    pub failing_after_change: Vec<String>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseResult {
    pub id: String,
    pub changed_files: usize,
    pub recommended: usize,
    pub total_test_files: usize,
    pub reduction: Option<f64>,
    pub failing: usize,
    /// Failing tests that appear in the recommendation list (known or guessed).
    pub found: usize,
    /// Failing tests found by a *known* (non-heuristic) relationship.
    pub found_known: usize,
    pub missed: Vec<String>,
    /// Missed tests that the tool does not even recognise as test files.
    pub missed_unrecognized: Vec<String>,
    pub selection_complete: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoricalResult {
    pub dataset_description: String,
    pub cases: Vec<CaseResult>,
    pub failing_total: usize,
    pub found_total: usize,
    pub found_known_total: usize,
    pub recall: Option<f64>,
    pub recall_ci95: Option<(f64, f64)>,
    pub recall_known_only: Option<f64>,
    pub cases_with_failures: usize,
    pub cases_fully_covered: usize,
    pub mean_reduction: Option<f64>,
    pub mean_reduction_when_complete: Option<f64>,
    pub caveats: Vec<String>,
}

pub fn load_dataset(path: &Path) -> Result<(Dataset, PathBuf)> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let ds: Dataset = serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    if ds.version != 1 {
        bail!("unsupported dataset version {} (expected 1)", ds.version);
    }
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let repo = {
        let p = Path::new(&ds.repo);
        if p.is_absolute() { p.to_path_buf() } else { base_dir.join(p) }
    };
    if !repo.join(".git").exists() && git(&repo, &["rev-parse", "--git-dir"]).is_err() {
        bail!("dataset repo {} is not a Git repository", repo.display());
    }
    Ok((ds, repo))
}

fn evaluate_case(repo: &Path, case: &Case) -> Result<CaseResult> {
    let tmp = tempfile::TempDir::new()?;
    let worktree = tmp.path().join("wt");
    let wt = worktree.to_string_lossy().to_string();
    git(repo, &["worktree", "add", "--detach", &wt, &case.head])
        .with_context(|| format!("case {}: could not check out {}", case.id, case.head))?;

    let analysis = (|| -> Result<CaseResult> {
        let mut cfg = AnalysisConfig::new(&worktree);
        cfg.base = Some(case.base.clone());
        let report = analyze(&cfg).with_context(|| format!("case {}: analysis failed", case.id))?;

        let recs = &report.test_recommendations.recommendations;
        let recommended: BTreeSet<&str> = recs.iter().map(|r| r.path.as_str()).collect();
        let known: BTreeSet<&str> = recs
            .iter()
            .filter(|r| r.basis == Basis::Known)
            .map(|r| r.path.as_str())
            .collect();
        let recognised: BTreeSet<&str> = report
            .graph
            .nodes
            .iter()
            .filter(|n| matches!(n.category, changegraph_core::discovery::FileCategory::Test))
            .map(|n| n.path.as_str())
            .collect();

        let failing: BTreeSet<&str> = case.failing_after_change.iter().map(String::as_str).collect();
        let missed: Vec<String> = failing
            .iter()
            .filter(|p| !recommended.contains(*p))
            .map(|p| p.to_string())
            .collect();
        let missed_unrecognized: Vec<String> = missed
            .iter()
            .filter(|p| !recognised.contains(p.as_str()))
            .cloned()
            .collect();
        let total = report.test_recommendations.summary.total_test_files;
        Ok(CaseResult {
            id: case.id.clone(),
            changed_files: report.changed_files.len(),
            recommended: recs.len(),
            total_test_files: total,
            reduction: reduction(recs.len(), total),
            failing: failing.len(),
            found: failing.len() - missed.len(),
            found_known: failing.iter().filter(|p| known.contains(*p)).count(),
            missed,
            missed_unrecognized,
            selection_complete: report.test_recommendations.completeness.complete,
        })
    })();

    // Always remove the worktree, even if the analysis failed.
    let _ = git(repo, &["worktree", "remove", "--force", &wt]);
    let _ = git(repo, &["worktree", "prune"]);
    analysis
}

pub fn evaluate(dataset_path: &Path) -> Result<HistoricalResult> {
    let (ds, repo) = load_dataset(dataset_path)?;
    let mut cases = Vec::new();
    for case in &ds.cases {
        cases.push(evaluate_case(&repo, case)?);
    }

    let failing_total: usize = cases.iter().map(|c| c.failing).sum();
    let found_total: usize = cases.iter().map(|c| c.found).sum();
    let found_known_total: usize = cases.iter().map(|c| c.found_known).sum();
    let mean = |v: Vec<f64>| (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64);

    let mut caveats = vec![
        format!(
            "{} case(s) and {} newly failing test file(s): small samples give wide intervals.",
            cases.len(),
            failing_total
        ),
        "Results describe this project's history and test conventions only.".to_string(),
        "'Newly failing at head' is a proxy for 'affected by the change'; flaky or environment-dependent \
         failures inflate the denominator, and regressions no test caught are invisible."
            .to_string(),
        "Recall of the full suite is 100% by definition; reduction is only meaningful together with recall."
            .to_string(),
    ];
    if failing_total == 0 {
        caveats.push("No failing tests in the dataset: recall is undefined, only reduction is reported.".to_string());
    }

    Ok(HistoricalResult {
        dataset_description: ds.description,
        failing_total,
        found_total,
        found_known_total,
        recall: (failing_total > 0).then(|| found_total as f64 / failing_total as f64),
        recall_ci95: wilson_interval(found_total, failing_total),
        recall_known_only: (failing_total > 0).then(|| found_known_total as f64 / failing_total as f64),
        cases_with_failures: cases.iter().filter(|c| c.failing > 0).count(),
        cases_fully_covered: cases.iter().filter(|c| c.failing > 0 && c.missed.is_empty()).count(),
        mean_reduction: mean(cases.iter().filter_map(|c| c.reduction).collect()),
        mean_reduction_when_complete: mean(
            cases
                .iter()
                .filter(|c| c.selection_complete)
                .filter_map(|c| c.reduction)
                .collect(),
        ),
        cases,
        caveats,
    })
}
