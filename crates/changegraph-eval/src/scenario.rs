//! Labeled end-to-end scenarios.
//!
//! A scenario copies a fixture project into a throwaway Git repository, commits it, applies a
//! change (modify / create / delete / rename), runs the real `analyze`, and compares the result
//! with hand-written labels. The labels say what a developer would conclude by reading the code:
//! which files import the changed code (directly or through others), and which tests do.
//!
//! These measure whether the implementation agrees with a careful human reading of *small*
//! projects. They do not measure regression-catching power.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use changegraph_core::test_mapping::Basis;
use changegraph_core::{analyze, AnalysisConfig, Report};
use serde::{Deserialize, Serialize};

use crate::gitutil::{commit_all, copy_dir, git, init_repo, write_file};
use crate::metrics::{compare, reduction, Counts, Score};

#[derive(Debug, Deserialize)]
pub struct ScenarioFile {
    pub scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub name: String,
    /// Fixture directory, relative to the repository root.
    pub fixture: String,
    /// Files written before the initial commit (e.g. `node_modules/...`, `dist/...`).
    #[serde(default)]
    pub extra_files: std::collections::BTreeMap<String, String>,
    /// Passed to the analyzer as `--exclude`.
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub changes: Vec<Change>,
    pub labels: Labels,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Change {
    Modify { path: String, append: String },
    Create { path: String, content: String },
    Delete { path: String },
    Rename { from: String, to: String },
}

#[derive(Debug, Deserialize)]
pub struct Labels {
    /// Files (other than the changed ones) that depend on the change, directly or transitively.
    pub affected: Vec<String>,
    /// Test files that import the changed code, or that changed themselves.
    pub relevant_tests: Vec<String>,
    /// Files the graph cannot connect to the change but that might depend on it.
    #[serde(default)]
    pub uncertain: Option<Vec<String>>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioResult {
    pub name: String,
    pub affected: Score,
    pub missing_affected: Vec<String>,
    pub unexpected_affected: Vec<String>,
    /// Recommended tests with a *known* basis versus `relevant_tests`.
    pub tests_known: Score,
    /// Every recommended test, including filename guesses.
    pub tests_all: Score,
    pub uncertain: Option<Score>,
    pub recommended: usize,
    pub total_test_files: usize,
    pub reduction: Option<f64>,
    pub selection_complete: bool,
    pub parse_clean_share: Option<f64>,
    pub unresolved_share: Option<f64>,
}

pub fn load_scenarios(dir: &Path) -> Result<Vec<Scenario>> {
    let mut files: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let text = fs::read_to_string(&f).with_context(|| format!("reading {}", f.display()))?;
        let parsed: ScenarioFile =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", f.display()))?;
        out.extend(parsed.scenarios);
    }
    Ok(out)
}

fn apply(dir: &Path, change: &Change) -> Result<()> {
    match change {
        Change::Modify { path, append } => {
            let p = dir.join(path);
            let mut text = fs::read_to_string(&p).with_context(|| format!("modify: reading {path}"))?;
            text.push_str(append);
            fs::write(&p, text)?;
        }
        Change::Create { path, content } => write_file(dir, path, content)?,
        Change::Delete { path } => fs::remove_file(dir.join(path)).with_context(|| format!("delete {path}"))?,
        Change::Rename { from, to } => {
            if let Some(parent) = dir.join(to).parent() {
                fs::create_dir_all(parent)?;
            }
            // `git mv` stages the rename so that git's rename detection sees both sides.
            git(dir, &["mv", from, to])?;
        }
    }
    Ok(())
}

fn paths<'a>(it: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    it.map(str::to_string).collect()
}

pub fn run_scenario(s: &Scenario, repo_root: &Path) -> Result<ScenarioResult> {
    let tmp = tempfile::TempDir::new()?;
    let dir = tmp.path();
    let fixture = repo_root.join(&s.fixture);
    if !fixture.is_dir() {
        bail!("scenario `{}`: fixture {} does not exist", s.name, fixture.display());
    }
    copy_dir(&fixture, dir)?;
    for (path, content) in &s.extra_files {
        write_file(dir, path, content)?;
    }
    init_repo(dir)?;
    commit_all(dir, "fixture")?;
    for c in &s.changes {
        apply(dir, c).with_context(|| format!("scenario `{}`", s.name))?;
    }

    let mut cfg = AnalysisConfig::new(dir);
    cfg.exclude = s.exclude.clone();
    let report = analyze(&cfg).with_context(|| format!("scenario `{}`: analysis failed", s.name))?;
    Ok(score(s, &report))
}

fn score(s: &Scenario, report: &Report) -> ScenarioResult {
    let tool_affected = paths(report.impact.affected.iter().map(|a| a.path.as_str()));
    let truth_affected = paths(s.labels.affected.iter().map(String::as_str));
    let recs = &report.test_recommendations.recommendations;
    let known = paths(recs.iter().filter(|r| r.basis == Basis::Known).map(|r| r.path.as_str()));
    let all = paths(recs.iter().map(|r| r.path.as_str()));
    let truth_tests = paths(s.labels.relevant_tests.iter().map(String::as_str));
    let uncertain = s.labels.uncertain.as_ref().map(|labels| {
        let tool = paths(report.impact.uncertain.iter().map(|u| u.path.as_str()));
        let truth = paths(labels.iter().map(String::as_str));
        Score::from(compare(&tool, &truth))
    });

    let g = &report.graph.stats;
    let imports = g.edges + g.external_dependencies + g.unresolved_imports;
    let total_tests = report.test_recommendations.summary.total_test_files;
    ScenarioResult {
        name: s.name.clone(),
        affected: Score::from(compare(&tool_affected, &truth_affected)),
        missing_affected: truth_affected.difference(&tool_affected).cloned().collect(),
        unexpected_affected: tool_affected.difference(&truth_affected).cloned().collect(),
        tests_known: Score::from(compare(&known, &truth_tests)),
        tests_all: Score::from(compare(&all, &truth_tests)),
        uncertain,
        recommended: recs.len(),
        total_test_files: total_tests,
        reduction: reduction(recs.len(), total_tests),
        selection_complete: report.test_recommendations.completeness.complete,
        parse_clean_share: (g.files > 0).then(|| g.parsed_ok as f64 / g.files as f64),
        unresolved_share: (imports > 0).then(|| g.unresolved_imports as f64 / imports as f64),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioAggregate {
    pub scenarios: usize,
    pub exact_affected: usize,
    pub exact_tests_known: usize,
    /// Micro-averaged over all scenarios (sums counts, then divides).
    pub affected: Score,
    pub tests_known: Score,
    pub tests_all: Score,
    pub mean_reduction: Option<f64>,
}

pub fn aggregate(results: &[ScenarioResult]) -> ScenarioAggregate {
    let (mut a, mut k, mut t) = (Counts::default(), Counts::default(), Counts::default());
    for r in results {
        a.add(&r.affected.counts());
        k.add(&r.tests_known.counts());
        t.add(&r.tests_all.counts());
    }
    let reductions: Vec<f64> = results.iter().filter_map(|r| r.reduction).collect();
    ScenarioAggregate {
        scenarios: results.len(),
        exact_affected: results.iter().filter(|r| r.affected.counts().is_exact()).count(),
        exact_tests_known: results.iter().filter(|r| r.tests_known.counts().is_exact()).count(),
        affected: a.into(),
        tests_known: k.into(),
        tests_all: t.into(),
        mean_reduction: (!reductions.is_empty()).then(|| reductions.iter().sum::<f64>() / reductions.len() as f64),
    }
}
