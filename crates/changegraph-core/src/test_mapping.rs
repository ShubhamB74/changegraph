//! Deterministic test recommendation.
//!
//! Given the impact analysis, recommend test files and explain *why* each was chosen, keeping
//! **known** relationships separate from **heuristic** guesses.
//!
//! # Methods (strongest first)
//!
//! * `changed_test` — the test file itself changed. Known.
//! * `direct_import` — the test imports a changed/affected module (a confirmed graph edge). Known.
//! * `dependency_reachability` — the test reaches a changed/affected module through a chain of
//!   confirmed imports. Known *graph* relationship.
//! * `coverage_map` — an optional, user-supplied map (see [`CoverageMap`]) says the test executed
//!   the module. Known *as far as the map is accurate and current*.
//! * `filename_heuristic` — fallback for impacted modules that have **no** known test; matches
//!   conventional names (`a.ts` ↔ `a.test.ts`, `__tests__/`, mirrored `src/`→`tests/`).
//!   A guess. Never counted as coverage.
//!
//! None of these proves that a test will catch a regression, and none proves that an unlisted
//! test would not. Dependency reachability in particular is over-approximate (file-level).
//!
//! # Honest incompleteness
//!
//! [`Completeness`] says when the list must not be trusted as complete: impacted modules with no
//! known test, depth-limited traversal, files whose dependencies are uncertain, unanalyzed
//! changed source files, changed configuration, a graph that does not match the target revision,
//! or an empty recommendation for a real change. "No tests recommended" is never reported as
//! "no tests needed".
//!
//! # Ordering
//!
//! basis (known first) → tier (changed test, graph-confirmed by distance to the nearest changed
//! module, coverage-map only, heuristic only) → more known covered modules → path.

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::dependency_graph::DependencyGraph;
use crate::discovery::FileCategory;
use crate::error::{ChangeGraphError, Result};
use crate::impact::{is_config_file, ChangedKind, IgnoredReason, ImpactReport};
use crate::report::{Warning, WarningCode};

pub const ORDERING: &str = "known basis first; then changed tests, graph-confirmed tests by distance to the \
nearest changed module, coverage-map-only tests, heuristic-only tests; then more covered modules; then path";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    ChangedTest,
    DirectImport,
    DependencyReachability,
    CoverageMap,
    FilenameHeuristic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// At least one non-heuristic relationship supports the recommendation.
    Known,
    /// Only filename heuristics support it.
    Heuristic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleStatus {
    Changed,
    Affected,
    /// Deleted, or the old path of a rename.
    Removed,
}

/// One relationship between a test and a changed/affected module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub module: String,
    pub module_status: ModuleStatus,
    pub method: Method,
    /// Number of imports from the test to the module (graph methods only).
    pub distance: Option<usize>,
    /// For transitive relations: the module the test imports directly on the way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRecommendation {
    pub rank: usize,
    pub path: String,
    pub basis: Basis,
    /// Distinct methods supporting this recommendation, strongest first.
    pub methods: Vec<Method>,
    /// Imports from the test to the nearest *changed* module, if graph-linked.
    pub min_distance: Option<usize>,
    pub covers: Vec<Coverage>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnmappedModule {
    pub path: String,
    pub status: ModuleStatus,
    /// Tests matched only by filename heuristics (not counted as coverage).
    pub heuristic_tests: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncompleteKind {
    UnmappedModules,
    DepthLimited,
    UncertainFiles,
    UnmodeledConfigChange,
    UnanalyzedChangedFiles,
    GraphTargetMismatch,
    NoTestsRecommended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IncompleteReason {
    pub kind: IncompleteKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Completeness {
    /// False whenever the recommended list may be missing tests that should run.
    pub complete: bool,
    pub reasons: Vec<IncompleteReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageMapInfo {
    pub path: String,
    pub tests: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestSummary {
    pub total_test_files: usize,
    pub recommended: usize,
    pub known_basis: usize,
    pub heuristic_only: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestReport {
    pub coverage_map: Option<CoverageMapInfo>,
    pub ordering: String,
    pub recommendations: Vec<TestRecommendation>,
    /// Impacted source modules with no test linked by a known method.
    pub unmapped_modules: Vec<UnmappedModule>,
    pub completeness: Completeness,
    pub summary: TestSummary,
}

// ----------------------------------------------------------------------------------------
// Optional coverage map
// ----------------------------------------------------------------------------------------

/// User-supplied per-test coverage:
///
/// ```json
/// { "version": 1, "tests": { "tests/a.test.ts": ["src/a.ts", "src/b.ts"] } }
/// ```
///
/// Paths are repository-relative. The tool does not validate the map against the revision;
/// a stale map yields stale recommendations. Per-test coverage must be produced by the user
/// (e.g. by running each test file under a coverage tool and listing the source files hit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageMap {
    pub source_path: String,
    pub tests: BTreeMap<String, Vec<String>>,
}

pub fn load_coverage_map(path: &Path) -> Result<CoverageMap> {
    let text = std::fs::read_to_string(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            ChangeGraphError::PathNotFound(path.to_path_buf())
        } else {
            ChangeGraphError::Io {
                path: path.to_path_buf(),
                source,
            }
        }
    })?;
    let tests =
        parse_coverage_map(&text).map_err(|message| ChangeGraphError::InvalidCoverageMap {
            path: path.to_path_buf(),
            message,
        })?;
    Ok(CoverageMap {
        source_path: path.display().to_string(),
        tests,
    })
}

fn normalize_path(p: &str) -> String {
    p.replace('\\', "/").trim_start_matches("./").to_string()
}

pub fn parse_coverage_map(
    text: &str,
) -> std::result::Result<BTreeMap<String, Vec<String>>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if value.get("version").and_then(|v| v.as_u64()) != Some(1) {
        return Err("`version` must be 1".to_string());
    }
    let tests = value
        .get("tests")
        .and_then(|t| t.as_object())
        .ok_or_else(|| {
            "`tests` must be an object mapping test paths to arrays of source paths".to_string()
        })?;
    let mut out = BTreeMap::new();
    for (test, files) in tests {
        let files = files
            .as_array()
            .ok_or_else(|| format!("`tests[\"{test}\"]` must be an array of strings"))?;
        let mut list = Vec::with_capacity(files.len());
        for f in files {
            let s = f
                .as_str()
                .ok_or_else(|| format!("`tests[\"{test}\"]` must contain only strings"))?;
            list.push(normalize_path(s));
        }
        out.insert(normalize_path(test), list);
    }
    Ok(out)
}

// ----------------------------------------------------------------------------------------
// Filename heuristic
// ----------------------------------------------------------------------------------------

fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

fn module_stem(file: &str) -> &str {
    let base = file.rsplit_once('.').map_or(file, |(s, _)| s);
    base.strip_suffix(".d").unwrap_or(base)
}

fn test_stem(file: &str) -> &str {
    let base = module_stem(file);
    base.strip_suffix(".test")
        .or_else(|| base.strip_suffix(".spec"))
        .unwrap_or(base)
}

/// If the first path segment of `dir` is one of `tops`, return the remainder.
fn strip_top<'a>(dir: &'a str, tops: &[&str]) -> Option<&'a str> {
    let (first, rest) = dir.split_once('/').unwrap_or((dir, ""));
    if !dir.is_empty() && tops.contains(&first) {
        Some(rest)
    } else {
        None
    }
}

/// Which naming convention (if any) links `test` to `module`. These are conventions, not proof.
pub fn heuristic_rule(module: &str, test: &str) -> Option<&'static str> {
    let (module_dir, module_file) = split_path(module);
    let (test_dir, test_file) = split_path(test);
    if module_stem(module_file) != test_stem(test_file) {
        return None;
    }
    if module_dir == test_dir {
        return Some("same directory and base name");
    }
    let sibling = if module_dir.is_empty() {
        "__tests__".to_string()
    } else {
        format!("{module_dir}/__tests__")
    };
    if test_dir == sibling {
        return Some("sibling __tests__ directory with the same base name");
    }
    let module_rel = strip_top(module_dir, &["src", "lib", "app"]);
    let test_rel = strip_top(test_dir, &["test", "tests", "__tests__", "spec"]);
    if let (Some(m), Some(t)) = (module_rel, test_rel) {
        if m == t {
            return Some("mirrored directory under a tests root with the same base name");
        }
    }
    None
}

// ----------------------------------------------------------------------------------------
// Recommendation
// ----------------------------------------------------------------------------------------

pub struct TestInput<'a> {
    pub graph: &'a DependencyGraph,
    pub impact: &'a ImpactReport,
    pub coverage: Option<&'a CoverageMap>,
    /// True if the graph was built from a checkout that is not the comparison target.
    pub graph_target_mismatch: bool,
}

#[derive(Default)]
struct Rec {
    changed_test: bool,
    covers: Vec<Coverage>,
}

/// Nodes reachable by following imports from `start`: `(node, distance, first_hop)`, where
/// `first_hop` is the module `start` imports directly on the shortest route.
fn forward_closure(graph: &DependencyGraph, start: usize) -> Vec<(usize, usize, usize)> {
    let n = graph.len();
    let mut dist = vec![usize::MAX; n];
    let mut first = vec![usize::MAX; n];
    let mut queue = VecDeque::from([start]);
    let mut out = Vec::new();
    dist[start] = 0;
    while let Some(v) = queue.pop_front() {
        for &w in graph.dependencies(v) {
            if dist[w] == usize::MAX {
                dist[w] = dist[v] + 1;
                first[w] = if v == start { w } else { first[v] };
                out.push((w, dist[w], first[w]));
                queue.push_back(w);
            }
        }
    }
    out
}

pub fn recommend_tests(input: &TestInput) -> (TestReport, Vec<Warning>) {
    let graph = input.graph;
    let impact = input.impact;
    let mut warnings = Vec::new();

    // 1. Impacted modules.
    let mut targets: HashMap<usize, ModuleStatus> = HashMap::new();
    let mut removed_paths: HashSet<String> = HashSet::new();
    let mut modules: BTreeMap<String, ModuleStatus> = BTreeMap::new(); // source modules needing a test
    let mut changed_tests: BTreeSet<usize> = BTreeSet::new();

    for c in &impact.changed {
        match c.kind {
            ChangedKind::Present => {
                if let Some(i) = graph.node_index(&c.path) {
                    targets.insert(i, ModuleStatus::Changed);
                    if c.category == FileCategory::Test {
                        changed_tests.insert(i);
                    }
                }
                if c.category == FileCategory::Source {
                    modules.insert(c.path.clone(), ModuleStatus::Changed);
                }
            }
            ChangedKind::Deleted | ChangedKind::RenamedFrom => {
                removed_paths.insert(c.path.clone());
                if c.category == FileCategory::Source {
                    modules.insert(c.path.clone(), ModuleStatus::Removed);
                }
            }
        }
    }
    for a in &impact.affected {
        if a.category == FileCategory::Source {
            if let Some(i) = graph.node_index(&a.path) {
                targets.entry(i).or_insert(ModuleStatus::Affected);
            }
            modules
                .entry(a.path.clone())
                .or_insert(ModuleStatus::Affected);
        }
    }

    let test_nodes: Vec<usize> = graph
        .nodes()
        .iter()
        .enumerate()
        .filter(|(_, n)| n.category == FileCategory::Test)
        .map(|(i, _)| i)
        .collect();

    let mut recs: BTreeMap<usize, Rec> = BTreeMap::new();

    // 2. Graph-confirmed relationships from changed and affected tests.
    let mut closure_tests: BTreeSet<usize> = changed_tests.clone();
    for t in &changed_tests {
        recs.entry(*t).or_default().changed_test = true;
    }
    for a in &impact.affected {
        if a.category == FileCategory::Test {
            if let Some(t) = graph.node_index(&a.path) {
                closure_tests.insert(t);
                recs.entry(t).or_default();
            }
        }
    }
    for &t in &closure_tests {
        let mut covers = Vec::new();
        for (w, dist, first) in forward_closure(graph, t) {
            let Some(&status) = targets.get(&w) else {
                continue;
            };
            let module = graph.node(w).path.clone();
            if dist == 1 {
                let detail = match graph.edge_between(t, w) {
                    Some(e) => format!("imports \"{}\" at line {}", e.specifier, e.line),
                    None => "imports it directly".to_string(),
                };
                covers.push(Coverage {
                    module,
                    module_status: status,
                    method: Method::DirectImport,
                    distance: Some(1),
                    via: None,
                    detail,
                });
            } else {
                let via = graph.node(first).path.clone();
                covers.push(Coverage {
                    detail: format!("reaches it through {via} ({dist} imports)"),
                    module,
                    module_status: status,
                    method: Method::DependencyReachability,
                    distance: Some(dist),
                    via: Some(via),
                });
            }
        }
        recs.entry(t).or_default().covers.extend(covers);
    }
    // Removed paths are not graph nodes; use the impact analysis' own distances.
    for a in &impact.affected {
        if a.category != FileCategory::Test {
            continue;
        }
        let Some(t) = graph.node_index(&a.path) else {
            continue;
        };
        for by in &a.affected_by {
            if removed_paths.contains(&by.changed_file) {
                recs.entry(t).or_default().covers.push(Coverage {
                    module: by.changed_file.clone(),
                    module_status: ModuleStatus::Removed,
                    method: if by.distance == 1 {
                        Method::DirectImport
                    } else {
                        Method::DependencyReachability
                    },
                    distance: Some(by.distance),
                    via: None,
                    detail: format!(
                        "depends on this removed path ({} imports; the import no longer resolves)",
                        by.distance
                    ),
                });
            }
        }
    }

    // 3. Optional coverage map.
    let mut map_info = None;
    if let Some(cm) = input.coverage {
        map_info = Some(CoverageMapInfo {
            path: cm.source_path.clone(),
            tests: cm.tests.len(),
        });
        let mut unknown_tests: Vec<&str> = Vec::new();
        for (test, files) in &cm.tests {
            let Some(t) = graph.node_index(test) else {
                unknown_tests.push(test);
                continue;
            };
            let mut covers = Vec::new();
            for f in files {
                let status = match graph.node_index(f) {
                    Some(i) => targets.get(&i).copied(),
                    None if removed_paths.contains(f) => Some(ModuleStatus::Removed),
                    None => None,
                };
                if let Some(module_status) = status {
                    covers.push(Coverage {
                        module: f.clone(),
                        module_status,
                        method: Method::CoverageMap,
                        distance: None,
                        via: None,
                        detail: "the coverage map lists this file as executed by the test"
                            .to_string(),
                    });
                }
            }
            if !covers.is_empty() {
                recs.entry(t).or_default().covers.extend(covers);
            }
        }
        if !unknown_tests.is_empty() {
            let shown: Vec<&str> = unknown_tests.iter().take(5).copied().collect();
            warnings.push(Warning {
                code: WarningCode::CoverageMapIssue,
                message: format!(
                    "{} test path(s) in the coverage map are not analyzed files and were ignored (e.g. {})",
                    unknown_tests.len(),
                    shown.join(", ")
                ),
            });
        }
    }

    // 4. Modules with no known test; heuristic fallback only for those.
    let covered: HashSet<String> = recs
        .values()
        .flat_map(|r| r.covers.iter())
        .map(|c| c.module.clone())
        .collect();
    let mut unmapped: Vec<UnmappedModule> = Vec::new();
    for (path, &status) in &modules {
        if covered.contains(path) {
            continue;
        }
        let mut heuristic_tests = Vec::new();
        for &t in &test_nodes {
            let test_path = &graph.node(t).path;
            if let Some(rule) = heuristic_rule(path, test_path) {
                recs.entry(t).or_default().covers.push(Coverage {
                    module: path.clone(),
                    module_status: status,
                    method: Method::FilenameHeuristic,
                    distance: None,
                    via: None,
                    detail: format!("filename heuristic: {rule}"),
                });
                heuristic_tests.push(test_path.clone());
            }
        }
        unmapped.push(UnmappedModule {
            path: path.clone(),
            status,
            heuristic_tests,
        });
    }

    // 5. Build, rank and explain.
    struct Built {
        key: (u8, u8, usize, Reverse<usize>, String),
        rec: TestRecommendation,
    }
    let mut built: Vec<Built> = Vec::new();
    for (t, mut rec) in recs {
        let mut methods: BTreeSet<Method> = rec.covers.iter().map(|c| c.method).collect();
        if rec.changed_test {
            methods.insert(Method::ChangedTest);
        }
        if methods.is_empty() {
            // Affected test with no recorded module link: still in the impact set.
            methods.insert(Method::DependencyReachability);
        }
        let basis = if methods.iter().any(|m| *m != Method::FilenameHeuristic) {
            Basis::Known
        } else {
            Basis::Heuristic
        };
        rec.covers.sort_by(|a, b| {
            (a.distance.unwrap_or(usize::MAX), &a.module, a.method).cmp(&(
                b.distance.unwrap_or(usize::MAX),
                &b.module,
                b.method,
            ))
        });
        let has_graph = methods.contains(&Method::DirectImport)
            || methods.contains(&Method::DependencyReachability);
        let min_distance = rec
            .covers
            .iter()
            .filter(|c| {
                matches!(
                    c.method,
                    Method::DirectImport | Method::DependencyReachability
                ) && c.module_status != ModuleStatus::Affected
            })
            .filter_map(|c| c.distance)
            .min();
        let tier: u8 = if rec.changed_test {
            0
        } else if has_graph {
            1
        } else if methods.contains(&Method::CoverageMap) {
            2
        } else {
            3
        };
        let known_cover_count = rec
            .covers
            .iter()
            .filter(|c| c.method != Method::FilenameHeuristic)
            .count();
        let path = graph.node(t).path.clone();
        let explanation = explain(rec.changed_test, &rec.covers, basis);
        built.push(Built {
            key: (
                if basis == Basis::Known { 0 } else { 1 },
                tier,
                min_distance.unwrap_or(usize::MAX),
                Reverse(known_cover_count),
                path.clone(),
            ),
            rec: TestRecommendation {
                rank: 0,
                path,
                basis,
                methods: methods.into_iter().collect(),
                min_distance,
                covers: rec.covers,
                explanation,
            },
        });
    }
    built.sort_by(|a, b| a.key.cmp(&b.key));
    let mut recommendations: Vec<TestRecommendation> = built.into_iter().map(|b| b.rec).collect();
    for (i, r) in recommendations.iter_mut().enumerate() {
        r.rank = i + 1;
    }

    // 6. Completeness.
    let mut reasons: Vec<IncompleteReason> = Vec::new();
    if !unmapped.is_empty() {
        let shown: Vec<&str> = unmapped.iter().take(5).map(|u| u.path.as_str()).collect();
        reasons.push(IncompleteReason {
            kind: IncompleteKind::UnmappedModules,
            detail: format!(
                "{} impacted source module(s) have no test linked by a known method (e.g. {}); \
                 their tests may not be in this list",
                unmapped.len(),
                shown.join(", ")
            ),
        });
    }
    if impact.depth_limited {
        reasons.push(IncompleteReason {
            kind: IncompleteKind::DepthLimited,
            detail: "impact traversal was cut off by --max-depth; tests beyond it are not listed"
                .to_string(),
        });
    }
    if !impact.uncertain.is_empty() {
        reasons.push(IncompleteReason {
            kind: IncompleteKind::UncertainFiles,
            detail: format!(
                "{} file(s) have unresolved imports or incomplete parses and may depend on changed code \
                 (including test files)",
                impact.uncertain.len()
            ),
        });
    }
    if impact
        .ignored_changed_files
        .iter()
        .any(|i| is_config_file(&i.path))
    {
        reasons.push(IncompleteReason {
            kind: IncompleteKind::UnmodeledConfigChange,
            detail:
                "a configuration or dependency file changed; its effect on tests is not modelled"
                    .to_string(),
        });
    }
    let unanalyzed = impact
        .ignored_changed_files
        .iter()
        .filter(|i| {
            matches!(
                i.reason,
                IgnoredReason::Excluded | IgnoredReason::NotInGraph
            )
        })
        .count();
    if unanalyzed > 0 {
        reasons.push(IncompleteReason {
            kind: IncompleteKind::UnanalyzedChangedFiles,
            detail: format!(
                "{unanalyzed} changed source file(s) were excluded or are not in the graph"
            ),
        });
    }
    if input.graph_target_mismatch {
        reasons.push(IncompleteReason {
            kind: IncompleteKind::GraphTargetMismatch,
            detail: "the dependency graph does not describe the comparison target revision"
                .to_string(),
        });
    }
    let has_real_change = impact.summary.changed_files > 0 || unanalyzed > 0;
    if has_real_change && recommendations.is_empty() {
        reasons.push(IncompleteReason {
            kind: IncompleteKind::NoTestsRecommended,
            detail: "no tests were identified for this change; this is NOT evidence that no tests are needed"
                .to_string(),
        });
    }
    let complete = reasons.is_empty();
    if !complete {
        let kinds: Vec<String> = reasons.iter().map(|r| format!("{:?}", r.kind)).collect();
        warnings.push(Warning {
            code: WarningCode::TestSelectionIncomplete,
            message: format!(
                "test selection is incomplete ({}); do not treat the recommended list as the full set of tests to run",
                kinds.join(", ")
            ),
        });
    }

    let known = recommendations
        .iter()
        .filter(|r| r.basis == Basis::Known)
        .count();
    let summary = TestSummary {
        total_test_files: test_nodes.len(),
        recommended: recommendations.len(),
        known_basis: known,
        heuristic_only: recommendations.len() - known,
    };

    (
        TestReport {
            coverage_map: map_info,
            ordering: ORDERING.to_string(),
            recommendations,
            unmapped_modules: unmapped,
            completeness: Completeness { complete, reasons },
            summary,
        },
        warnings,
    )
}

fn explain(changed_test: bool, covers: &[Coverage], basis: Basis) -> String {
    let mut parts: Vec<String> = Vec::new();
    if changed_test {
        parts.push("the test file itself changed".to_string());
    }
    let graph: Vec<&Coverage> = covers
        .iter()
        .filter(|c| {
            matches!(
                c.method,
                Method::DirectImport | Method::DependencyReachability
            )
        })
        .collect();
    if let Some(first) = graph.first() {
        let how = match first.distance {
            Some(1) => "direct import".to_string(),
            Some(d) => format!("{d} imports away"),
            None => "reachable".to_string(),
        };
        parts.push(format!(
            "depends on {} changed/affected module(s); closest: {} ({how})",
            graph.len(),
            first.module
        ));
    }
    let mapped = covers
        .iter()
        .filter(|c| c.method == Method::CoverageMap)
        .count();
    if mapped > 0 {
        parts.push(format!(
            "the coverage map links it to {mapped} impacted module(s)"
        ));
    }
    if let Some(h) = covers
        .iter()
        .find(|c| c.method == Method::FilenameHeuristic)
    {
        let prefix = if basis == Basis::Heuristic {
            "FILENAME HEURISTIC ONLY, not a proven relationship: "
        } else {
            "also matched by naming convention: "
        };
        parts.push(format!("{prefix}{} ({})", h.module, h.detail));
    }
    if parts.is_empty() {
        parts.push("it is in the impact set (see impact.affected)".to_string());
    }
    let label = match basis {
        Basis::Known => "Known relationship",
        Basis::Heuristic => "Heuristic guess",
    };
    format!("{label}: {}.", parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_graph::{DependencyEdge, GraphNode, UnresolvedImport};
    use crate::discovery::classify;
    use crate::git_diff::{ChangeStatus, ChangedFile};
    use crate::impact::analyze_impact;
    use crate::parser::{ImportKind, ParseStatus};
    use crate::resolver::UnresolvedReason;

    fn node(path: &str) -> GraphNode {
        GraphNode {
            path: path.to_string(),
            category: classify(path),
            parse_status: ParseStatus::Ok,
            exports: Vec::new(),
        }
    }

    fn edge(from: &str, to: &str) -> DependencyEdge {
        DependencyEdge {
            from: from.to_string(),
            to: to.to_string(),
            kind: ImportKind::Import,
            type_only: false,
            specifier: format!(
                "./{}",
                to.rsplit('/').next().unwrap().trim_end_matches(".ts")
            ),
            line: 1,
        }
    }

    fn graph(
        names: &[&str],
        edges: &[(&str, &str)],
        unresolved: Vec<UnresolvedImport>,
    ) -> DependencyGraph {
        DependencyGraph::from_parts(
            names.iter().map(|n| node(n)).collect(),
            edges.iter().map(|(a, b)| edge(a, b)).collect(),
            vec![],
            unresolved,
        )
    }

    fn modified(path: &str) -> ChangedFile {
        ChangedFile::new(path.to_string(), None, ChangeStatus::Modified)
    }

    fn recommend(
        g: &DependencyGraph,
        changed: &[ChangedFile],
        depth: Option<usize>,
        cov: Option<&CoverageMap>,
    ) -> (TestReport, Vec<Warning>) {
        let (impact, _) = analyze_impact(g, changed, None, depth);
        recommend_tests(&TestInput {
            graph: g,
            impact: &impact,
            coverage: cov,
            graph_target_mismatch: false,
        })
    }

    fn paths(r: &TestReport) -> Vec<&str> {
        r.recommendations.iter().map(|t| t.path.as_str()).collect()
    }

    #[test]
    fn heuristic_rules() {
        assert!(heuristic_rule("src/a.ts", "src/a.test.ts").is_some());
        assert!(heuristic_rule("src/a.ts", "src/__tests__/a.spec.ts").is_some());
        assert!(heuristic_rule("src/api/a.ts", "tests/api/a.test.ts").is_some());
        assert!(heuristic_rule("src/a.ts", "tests/a.test.ts").is_some());
        assert!(heuristic_rule("a.ts", "a.test.ts").is_some());
        assert!(heuristic_rule("src/a.d.ts", "src/a.test.ts").is_some());
        // Different base name, or a directory layout we do not recognise.
        assert!(heuristic_rule("src/a.ts", "src/b.test.ts").is_none());
        assert!(heuristic_rule("src/api/a.ts", "tests/a.test.ts").is_none());
        assert!(heuristic_rule("packages/x/a.ts", "other/a.test.ts").is_none());
    }

    #[test]
    fn known_recommendations_are_ranked_by_distance_to_the_changed_module() {
        let g = graph(
            &[
                "src/a.ts",
                "src/b.ts",
                "tests/a.test.ts",
                "tests/b.test.ts",
                "tests/x.test.ts",
                "src/z.ts",
            ],
            &[
                ("src/b.ts", "src/a.ts"),
                ("tests/a.test.ts", "src/a.ts"),
                ("tests/b.test.ts", "src/b.ts"),
                ("tests/x.test.ts", "src/z.ts"),
            ],
            vec![],
        );
        let (r, w) = recommend(&g, &[modified("src/a.ts")], None, None);
        assert_eq!(paths(&r), vec!["tests/a.test.ts", "tests/b.test.ts"]);
        assert!(r.completeness.complete, "{:?}", r.completeness);
        assert!(w.is_empty());
        assert!(r.recommendations.iter().all(|t| t.basis == Basis::Known));

        let a = &r.recommendations[0];
        assert_eq!(a.methods, vec![Method::DirectImport]);
        assert_eq!(a.min_distance, Some(1));
        assert!(a.covers[0].detail.contains("imports \"./a\" at line 1"));
        assert!(a.explanation.starts_with("Known relationship"));

        let b = &r.recommendations[1];
        assert_eq!(b.min_distance, Some(2));
        let reach = b.covers.iter().find(|c| c.module == "src/a.ts").unwrap();
        assert_eq!(reach.method, Method::DependencyReachability);
        assert_eq!(reach.via.as_deref(), Some("src/b.ts"));
        assert_eq!(reach.module_status, ModuleStatus::Changed);
        let direct = b.covers.iter().find(|c| c.module == "src/b.ts").unwrap();
        assert_eq!(direct.module_status, ModuleStatus::Affected);

        assert_eq!(r.summary.total_test_files, 3);
        assert_eq!(r.summary.recommended, 2);
        assert!(r.unmapped_modules.is_empty());
    }

    #[test]
    fn missing_mapping_never_reads_as_no_tests_needed() {
        let g = graph(
            &["src/a.ts", "src/b.ts"],
            &[("src/b.ts", "src/a.ts")],
            vec![],
        );
        let (r, w) = recommend(&g, &[modified("src/a.ts")], None, None);
        assert!(r.recommendations.is_empty());
        let unmapped: Vec<_> = r.unmapped_modules.iter().map(|u| u.path.as_str()).collect();
        assert_eq!(unmapped, vec!["src/a.ts", "src/b.ts"]);
        assert!(!r.completeness.complete);
        let kinds: Vec<_> = r.completeness.reasons.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&IncompleteKind::UnmappedModules));
        assert!(kinds.contains(&IncompleteKind::NoTestsRecommended));
        assert!(w
            .iter()
            .any(|w| w.code == WarningCode::TestSelectionIncomplete));
    }

    #[test]
    fn heuristic_matches_are_labeled_and_do_not_count_as_coverage() {
        // tests/util.test.ts does not import src/util.ts; only the name matches.
        let g = graph(&["src/util.ts", "tests/util.test.ts"], &[], vec![]);
        let (r, _) = recommend(&g, &[modified("src/util.ts")], None, None);
        assert_eq!(paths(&r), vec!["tests/util.test.ts"]);
        let t = &r.recommendations[0];
        assert_eq!(t.basis, Basis::Heuristic);
        assert_eq!(t.methods, vec![Method::FilenameHeuristic]);
        assert!(t.explanation.contains("FILENAME HEURISTIC ONLY"));
        assert_eq!(r.unmapped_modules[0].path, "src/util.ts");
        assert_eq!(
            r.unmapped_modules[0].heuristic_tests,
            vec!["tests/util.test.ts"]
        );
        assert!(!r.completeness.complete);
        assert_eq!(r.summary.heuristic_only, 1);
    }

    #[test]
    fn heuristics_are_not_used_for_modules_that_already_have_a_known_test() {
        // a.test.ts imports a.ts (known). other.test.ts shares only a *name* match with a.ts's
        // module b? No: here "src/a.ts" is covered, so tests/a.test.ts's twin must not be added.
        let g = graph(
            &["src/a.ts", "tests/a.test.ts", "src/__tests__/a.test.ts"],
            &[("tests/a.test.ts", "src/a.ts")],
            vec![],
        );
        let (r, _) = recommend(&g, &[modified("src/a.ts")], None, None);
        assert_eq!(paths(&r), vec!["tests/a.test.ts"]);
        assert!(r.unmapped_modules.is_empty());
    }

    #[test]
    fn a_changed_test_ranks_first_and_is_known() {
        let g = graph(
            &["src/a.ts", "tests/a.test.ts", "tests/c.test.ts"],
            &[
                ("tests/a.test.ts", "src/a.ts"),
                ("tests/c.test.ts", "src/a.ts"),
            ],
            vec![],
        );
        let (r, _) = recommend(
            &g,
            &[modified("src/a.ts"), modified("tests/c.test.ts")],
            None,
            None,
        );
        assert_eq!(paths(&r), vec!["tests/c.test.ts", "tests/a.test.ts"]);
        assert!(r.recommendations[0].methods.contains(&Method::ChangedTest));
        assert!(r.recommendations[0]
            .explanation
            .contains("the test file itself changed"));
    }

    #[test]
    fn a_changed_test_with_no_other_link_is_still_recommended() {
        let g = graph(&["tests/c.test.ts"], &[], vec![]);
        let (r, _) = recommend(&g, &[modified("tests/c.test.ts")], None, None);
        assert_eq!(paths(&r), vec!["tests/c.test.ts"]);
        assert_eq!(r.recommendations[0].methods, vec![Method::ChangedTest]);
        assert!(r.completeness.complete);
    }

    #[test]
    fn coverage_map_adds_known_recommendations_and_warns_about_unknown_tests() {
        let g = graph(&["src/a.ts", "tests/m.test.ts"], &[], vec![]);
        let mut tests = BTreeMap::new();
        tests.insert(
            "tests/m.test.ts".to_string(),
            vec!["src/a.ts".to_string(), "src/untouched.ts".to_string()],
        );
        tests.insert(
            "tests/ghost.test.ts".to_string(),
            vec!["src/a.ts".to_string()],
        );
        let cm = CoverageMap {
            source_path: "cov.json".into(),
            tests,
        };
        let (r, w) = recommend(&g, &[modified("src/a.ts")], None, Some(&cm));

        assert_eq!(paths(&r), vec!["tests/m.test.ts"]);
        let t = &r.recommendations[0];
        assert_eq!(t.basis, Basis::Known);
        assert_eq!(t.methods, vec![Method::CoverageMap]);
        assert_eq!(t.covers.len(), 1);
        assert!(r.unmapped_modules.is_empty());
        assert!(r.completeness.complete);
        assert_eq!(r.coverage_map.as_ref().unwrap().tests, 2);
        assert!(w
            .iter()
            .any(|w| w.code == WarningCode::CoverageMapIssue && w.message.contains("ghost")));
    }

    #[test]
    fn deleted_modules_pull_in_tests_that_depend_on_the_removed_path() {
        // b imports ./a (deleted); tests/b.test.ts imports b.
        let unresolved = vec![UnresolvedImport {
            from: "src/b.ts".into(),
            specifier: "./a".into(),
            reason: UnresolvedReason::NotFound,
            kind: ImportKind::Import,
            type_only: false,
            line: 1,
        }];
        let g = graph(
            &["src/b.ts", "tests/b.test.ts"],
            &[("tests/b.test.ts", "src/b.ts")],
            unresolved,
        );
        let deleted = ChangedFile::new("src/a.ts".into(), None, ChangeStatus::Deleted);
        let (r, _) = recommend(&g, &[deleted], None, None);
        assert_eq!(paths(&r), vec!["tests/b.test.ts"]);
        let t = &r.recommendations[0];
        let removed = t.covers.iter().find(|c| c.module == "src/a.ts").unwrap();
        assert_eq!(removed.module_status, ModuleStatus::Removed);
        assert_eq!(removed.method, Method::DependencyReachability);
        assert_eq!(removed.distance, Some(2));
        assert!(r.unmapped_modules.is_empty(), "{:?}", r.unmapped_modules);
    }

    #[test]
    fn depth_limit_and_uncertain_files_make_selection_incomplete() {
        let mut partial = node("src/p.ts");
        partial.parse_status = ParseStatus::Partial;
        let g = DependencyGraph::from_parts(
            vec![
                node("src/a.ts"),
                node("src/b.ts"),
                partial,
                node("tests/b.test.ts"),
            ],
            vec![
                edge("src/b.ts", "src/a.ts"),
                edge("tests/b.test.ts", "src/b.ts"),
            ],
            vec![],
            vec![],
        );
        let (r, _) = recommend(&g, &[modified("src/a.ts")], Some(1), None);
        let kinds: Vec<_> = r.completeness.reasons.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&IncompleteKind::DepthLimited));
        assert!(kinds.contains(&IncompleteKind::UncertainFiles));
        assert!(!r.completeness.complete);
    }

    #[test]
    fn unanalyzed_source_changes_and_config_changes_are_flagged() {
        let g = graph(
            &["src/a.ts", "tests/a.test.ts"],
            &[("tests/a.test.ts", "src/a.ts")],
            vec![],
        );
        let changed = vec![
            modified("src/a.ts"),
            modified("src/ghost.ts"),
            modified("package.json"),
        ];
        let (r, _) = recommend(&g, &changed, None, None);
        let kinds: Vec<_> = r.completeness.reasons.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&IncompleteKind::UnanalyzedChangedFiles));
        assert!(kinds.contains(&IncompleteKind::UnmodeledConfigChange));
        assert_eq!(paths(&r), vec!["tests/a.test.ts"]);
    }

    #[test]
    fn docs_only_changes_do_not_cry_wolf_and_graph_mismatch_is_flagged() {
        let g = graph(&["src/a.ts"], &[], vec![]);
        let (impact, _) = analyze_impact(&g, &[modified("README.md")], None, None);
        let (r, w) = recommend_tests(&TestInput {
            graph: &g,
            impact: &impact,
            coverage: None,
            graph_target_mismatch: false,
        });
        assert!(r.completeness.complete && r.recommendations.is_empty() && w.is_empty());

        let (r2, _) = recommend_tests(&TestInput {
            graph: &g,
            impact: &impact,
            coverage: None,
            graph_target_mismatch: true,
        });
        assert_eq!(
            r2.completeness.reasons[0].kind,
            IncompleteKind::GraphTargetMismatch
        );
    }

    #[test]
    fn coverage_map_parsing_validates_the_format() {
        let ok = parse_coverage_map(r#"{"version":1,"tests":{"./t\\a.test.ts":["./src/a.ts"]}}"#)
            .unwrap();
        assert_eq!(ok["t/a.test.ts"], vec!["src/a.ts"]);
        assert!(parse_coverage_map(r#"{"version":2,"tests":{}}"#).is_err());
        assert!(parse_coverage_map(r#"{"version":1}"#).is_err());
        assert!(parse_coverage_map(r#"{"version":1,"tests":{"a":"b"}}"#).is_err());
        assert!(parse_coverage_map(r#"{"version":1,"tests":{"a":[1]}}"#).is_err());
        assert!(parse_coverage_map("not json").is_err());
    }

    #[test]
    fn recommendations_are_deterministic() {
        let g = graph(
            &["src/a.ts", "tests/a.test.ts", "tests/b.test.ts"],
            &[
                ("tests/a.test.ts", "src/a.ts"),
                ("tests/b.test.ts", "src/a.ts"),
            ],
            vec![],
        );
        let (r1, _) = recommend(&g, &[modified("src/a.ts")], None, None);
        let (r2, _) = recommend(&g, &[modified("src/a.ts")], None, None);
        assert_eq!(r1, r2);
        assert_eq!(paths(&r1), vec!["tests/a.test.ts", "tests/b.test.ts"]);
    }
}
