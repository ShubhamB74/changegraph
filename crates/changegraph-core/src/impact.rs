//! Change-impact analysis.
//!
//! Given the dependency graph (`A -> B` = A imports B) and the changed files, compute which
//! other files may be affected by walking edges **in reverse**: a change to B can affect
//! every file that imports B, directly or transitively.
//!
//! # Classification
//!
//! * **changed** — the files that changed (plus deleted/renamed-away paths, see below).
//! * **direct** — imports a changed file (distance 1).
//! * **transitive** — reaches a changed file through two or more imports (distance ≥ 2).
//! * **uncertain** — *not* reachable in the graph, but has unresolved imports or an
//!   incomplete parse, so it may depend on changed code in a way the graph cannot see.
//!   These are reported separately and are never mixed into the confirmed results.
//!
//! # Deleted and renamed files
//!
//! A deleted file (or the old path of a rename) is not a graph node, and files that still
//! import it show up as *unresolved* imports. To avoid the false assurance that deleting a
//! file affects nothing, unresolved imports are re-resolved against the removed paths; any
//! importer that matches becomes a direct dependent of the removed file.
//!
//! # Ranking (transparent, not a probability)
//!
//! Affected files are ordered by: (1) shortest distance to any changed file, ascending;
//! (2) number of distinct changed files that reach it, descending; (3) path, ascending.
//! No numeric "risk score" is produced.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::dependency_graph::{DependencyGraph, UnresolvedImport};
use crate::discovery::{classify, FileCategory};
use crate::git_diff::{ChangeStatus, ChangedFile};
use crate::parser::{ImportKind, ParseStatus};
use crate::report::{Warning, WarningCode};
use crate::resolver::{ModuleResolver, Resolution, UnresolvedReason};
use crate::tsconfig::TsConfig;

pub const ORDERING: &str = "distance asc, changed_dependency_count desc, path asc";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImpactClass {
    Direct,
    Transitive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangedKind {
    /// The file exists in the graph.
    Present,
    /// The file was deleted (not in the graph).
    Deleted,
    /// The old path of a rename (not in the graph).
    RenamedFrom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoredReason {
    /// Not a TypeScript/JavaScript file, so it has no node in the graph.
    NonSourceFile,
    /// Matches an exclusion pattern.
    Excluded,
    /// A source file that is missing from the graph (e.g. the working tree is not at the
    /// target revision).
    NotInGraph,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyKind {
    UnresolvedImport,
    ParsePartial,
    ParseFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedNode {
    pub path: String,
    pub status: ChangeStatus,
    pub category: FileCategory,
    pub kind: ChangedKind,
    /// Affected files at distance 1 from this change (changed files are excluded).
    pub direct_dependents: usize,
    /// Affected files at distance ≥ 2 from this change.
    pub transitive_dependents: usize,
}

/// One import statement on an impact path: `importer` imports `imported`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hop {
    pub importer: String,
    pub imported: String,
    pub kind: ImportKind,
    pub type_only: bool,
    pub specifier: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AffectedBy {
    pub changed_file: String,
    pub distance: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AffectedNode {
    /// 1-based position in the documented ordering.
    pub rank: usize,
    pub path: String,
    pub category: FileCategory,
    pub classification: ImpactClass,
    /// Shortest distance to any changed file.
    pub distance: usize,
    /// Number of distinct changed files that reach this file.
    pub changed_dependency_count: usize,
    /// Every changed file that reaches this one, with its shortest distance.
    pub affected_by: Vec<AffectedBy>,
    /// Shortest path, starting at the changed file and ending at this file.
    pub path_from_changed: Vec<String>,
    /// The import statements along `path_from_changed`, in the same order.
    pub hops: Vec<Hop>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UncertaintyReason {
    pub kind: UncertaintyKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub specifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UncertainNode {
    pub path: String,
    pub category: FileCategory,
    pub reasons: Vec<UncertaintyReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IgnoredChange {
    pub path: String,
    pub reason: IgnoredReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactSummary {
    /// Changed items that were analyzed (present + deleted/renamed-away).
    pub changed_files: usize,
    pub affected: usize,
    pub direct: usize,
    pub transitive: usize,
    pub affected_tests: usize,
    pub uncertain: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImpactReport {
    pub max_depth: Option<usize>,
    /// True if traversal stopped at `max_depth` while further dependents existed.
    pub depth_limited: bool,
    pub ordering: String,
    pub changed: Vec<ChangedNode>,
    pub affected: Vec<AffectedNode>,
    pub uncertain: Vec<UncertainNode>,
    /// Changed files that were not analyzed, each with a reason. Nothing is dropped silently.
    pub ignored_changed_files: Vec<IgnoredChange>,
    pub summary: ImpactSummary,
}

struct Dangling {
    specifier: String,
    kind: ImportKind,
    type_only: bool,
    line: u32,
}

struct Seed {
    label: String,
    kind: ChangedKind,
    status: ChangeStatus,
    category: FileCategory,
    /// Graph node for present files.
    node: Option<usize>,
    /// BFS starting points as `(node, distance)`.
    starts: Vec<(usize, usize)>,
    /// For removed paths: importer node -> the import that referenced the removed path.
    dangling: HashMap<usize, Dangling>,
}

struct Best {
    dist: usize,
    seed: usize,
    /// Nodes from the BFS start to the affected node, inclusive.
    chain: Vec<usize>,
}

struct Bfs {
    /// Nodes in discovery order, starting points first.
    order: Vec<usize>,
    dist: Vec<usize>,
    parent: Vec<usize>,
    truncated: bool,
}

/// Cycle-safe reverse BFS from `starts`. Neighbour lists are sorted, so discovery order
/// (and therefore the shortest path chosen) is deterministic.
fn reverse_bfs(graph: &DependencyGraph, starts: &[(usize, usize)], max_depth: Option<usize>) -> Bfs {
    let n = graph.len();
    let mut dist = vec![usize::MAX; n];
    let mut parent = vec![usize::MAX; n];
    let mut order = Vec::new();
    let mut queue = VecDeque::new();
    let mut truncated = false;

    for &(node, d) in starts {
        if max_depth.is_some_and(|m| d > m) {
            truncated = true;
            continue;
        }
        if dist[node] == usize::MAX {
            dist[node] = d;
            order.push(node);
            queue.push_back(node);
        }
    }
    while let Some(v) = queue.pop_front() {
        let d = dist[v];
        if max_depth.is_some_and(|m| d >= m) {
            if graph.dependents(v).iter().any(|&w| dist[w] == usize::MAX) {
                truncated = true;
            }
            continue;
        }
        for &w in graph.dependents(v) {
            if dist[w] == usize::MAX {
                dist[w] = d + 1;
                parent[w] = v;
                order.push(w);
                queue.push_back(w);
            }
        }
    }
    Bfs {
        order,
        dist,
        parent,
        truncated,
    }
}

fn is_config_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "package.json" | "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "jsconfig.json"
    ) || (name.starts_with("tsconfig") && name.ends_with(".json"))
}

/// Compute the impact of `changed_files` on `graph`.
pub fn analyze_impact(
    graph: &DependencyGraph,
    changed_files: &[ChangedFile],
    tsconfig: Option<&TsConfig>,
    max_depth: Option<usize>,
) -> (ImpactReport, Vec<Warning>) {
    let mut warnings = Vec::new();
    let mut ignored: Vec<IgnoredChange> = Vec::new();
    let mut present: Vec<(usize, &ChangedFile)> = Vec::new();
    let mut removed: Vec<(String, ChangedKind, ChangeStatus, FileCategory)> = Vec::new();

    // 1. Sort every changed file into present / removed / ignored.
    for f in changed_files {
        if f.category == FileCategory::Other {
            ignored.push(IgnoredChange {
                path: f.path.clone(),
                reason: IgnoredReason::NonSourceFile,
            });
            if is_config_file(&f.path) {
                warnings.push(Warning {
                    code: WarningCode::UnmodeledConfigChange,
                    message: format!(
                        "{} changed; configuration and dependency changes can affect many files \
                         and are not modelled by the dependency graph",
                        f.path
                    ),
                });
            }
            continue;
        }
        if f.excluded {
            ignored.push(IgnoredChange {
                path: f.path.clone(),
                reason: IgnoredReason::Excluded,
            });
            continue;
        }
        match (f.status, graph.node_index(&f.path)) {
            (_, Some(i)) => present.push((i, f)),
            (ChangeStatus::Deleted, None) => {
                removed.push((f.path.clone(), ChangedKind::Deleted, f.status, f.category))
            }
            (_, None) => ignored.push(IgnoredChange {
                path: f.path.clone(),
                reason: IgnoredReason::NotInGraph,
            }),
        }
        if f.status == ChangeStatus::Renamed {
            if let Some(old) = &f.old_path {
                let category = classify(old);
                if category != FileCategory::Other && graph.node_index(old).is_none() {
                    removed.push((old.clone(), ChangedKind::RenamedFrom, f.status, category));
                }
            }
        }
    }
    present.sort_unstable_by_key(|&(i, _)| i);
    present.dedup_by_key(|&mut (i, _)| i);
    removed.sort_by(|a, b| a.0.cmp(&b.0));
    removed.dedup_by(|a, b| a.0 == b.0);
    ignored.sort_by(|a, b| a.path.cmp(&b.path));

    // 2. Importers whose (currently unresolved) imports point at a removed path.
    let mut dangling_by_path: HashMap<String, HashMap<usize, Dangling>> = HashMap::new();
    let mut dangling_keys: HashSet<(String, String, u32)> = HashSet::new();
    if !removed.is_empty() {
        let mut known: HashSet<String> = graph.nodes().iter().map(|n| n.path.clone()).collect();
        known.extend(removed.iter().map(|r| r.0.clone()));
        let resolver = ModuleResolver::new(&known, tsconfig);
        for u in graph.unresolved_imports() {
            if u.reason != UnresolvedReason::NotFound {
                continue;
            }
            let Resolution::Internal(target) = resolver.resolve(&u.from, &u.specifier) else {
                continue;
            };
            if !removed.iter().any(|r| r.0 == target) {
                continue;
            }
            let Some(importer) = graph.node_index(&u.from) else {
                continue;
            };
            dangling_keys.insert((u.from.clone(), u.specifier.clone(), u.line));
            dangling_by_path
                .entry(target)
                .or_default()
                .entry(importer)
                .or_insert(Dangling {
                    specifier: u.specifier.clone(),
                    kind: u.kind,
                    type_only: u.type_only,
                    line: u.line,
                });
        }
    }

    // 3. Seeds: present files first (by node index), then removed paths (by path).
    let mut seeds: Vec<Seed> = Vec::new();
    for &(i, f) in &present {
        seeds.push(Seed {
            label: f.path.clone(),
            kind: ChangedKind::Present,
            status: f.status,
            category: f.category,
            node: Some(i),
            starts: vec![(i, 0)],
            dangling: HashMap::new(),
        });
    }
    for (path, kind, status, category) in removed {
        let dangling = dangling_by_path.remove(&path).unwrap_or_default();
        let mut starts: Vec<(usize, usize)> = dangling.keys().map(|&n| (n, 1)).collect();
        starts.sort_unstable();
        seeds.push(Seed {
            label: path,
            kind,
            status,
            category,
            node: None,
            starts,
            dangling,
        });
    }

    // 4. One reverse BFS per seed; keep the globally shortest path for each node.
    let n = graph.len();
    let changed_nodes: HashSet<usize> = seeds.iter().filter_map(|s| s.node).collect();
    let mut best: Vec<Option<Best>> = (0..n).map(|_| None).collect();
    let mut by: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    let mut counts: Vec<(usize, usize)> = Vec::with_capacity(seeds.len());
    let mut depth_limited = false;

    for (si, seed) in seeds.iter().enumerate() {
        let bfs = reverse_bfs(graph, &seed.starts, max_depth);
        depth_limited |= bfs.truncated;
        let (mut direct, mut transitive) = (0usize, 0usize);
        for &v in &bfs.order {
            if changed_nodes.contains(&v) {
                continue;
            }
            let d = bfs.dist[v];
            if d == 1 {
                direct += 1;
            } else {
                transitive += 1;
            }
            by[v].push((si, d));
            if best[v].as_ref().is_none_or(|b| d < b.dist) {
                let mut chain = vec![v];
                let mut cur = v;
                while bfs.parent[cur] != usize::MAX {
                    cur = bfs.parent[cur];
                    chain.push(cur);
                }
                chain.reverse();
                best[v] = Some(Best {
                    dist: d,
                    seed: si,
                    chain,
                });
            }
        }
        counts.push((direct, transitive));
    }

    // 5. Assemble affected nodes, ranked.
    let mut affected: Vec<AffectedNode> = Vec::new();
    for (v, slot) in best.iter().enumerate() {
        let Some(b) = slot else { continue };
        let seed = &seeds[b.seed];
        let (path_from_changed, hops) = build_path(graph, seed, b);
        let mut affected_by: Vec<AffectedBy> = by[v]
            .iter()
            .map(|&(si, d)| AffectedBy {
                changed_file: seeds[si].label.clone(),
                distance: d,
            })
            .collect();
        affected_by.sort_by(|a, b| (a.distance, &a.changed_file).cmp(&(b.distance, &b.changed_file)));
        affected.push(AffectedNode {
            rank: 0,
            path: graph.node(v).path.clone(),
            category: graph.node(v).category,
            classification: if b.dist == 1 {
                ImpactClass::Direct
            } else {
                ImpactClass::Transitive
            },
            distance: b.dist,
            changed_dependency_count: by[v].len(),
            affected_by,
            explanation: explain(seed, &hops, b.dist),
            path_from_changed,
            hops,
        });
    }
    affected.sort_by(|a, b| {
        (a.distance, Reverse(a.changed_dependency_count), &a.path).cmp(&(
            b.distance,
            Reverse(b.changed_dependency_count),
            &b.path,
        ))
    });
    for (i, item) in affected.iter_mut().enumerate() {
        item.rank = i + 1;
    }

    // 6. Uncertain: unreachable files whose unresolved imports / partial parses may hide a link.
    let mut uncertain: Vec<UncertainNode> = Vec::new();
    if !seeds.is_empty() {
        let mut unresolved_by: HashMap<usize, Vec<&UnresolvedImport>> = HashMap::new();
        for u in graph.unresolved_imports() {
            // Asset imports (.json, .css, ...) cannot hide a code dependency.
            if u.reason == UnresolvedReason::UnsupportedFileType {
                continue;
            }
            // Already explained as a dependent of a removed file.
            if dangling_keys.contains(&(u.from.clone(), u.specifier.clone(), u.line)) {
                continue;
            }
            if let Some(i) = graph.node_index(&u.from) {
                unresolved_by.entry(i).or_default().push(u);
            }
        }
        for (v, slot) in best.iter().enumerate() {
            if changed_nodes.contains(&v) || slot.is_some() {
                continue;
            }
            let node = graph.node(v);
            let mut reasons = Vec::new();
            match node.parse_status {
                ParseStatus::Partial => reasons.push(UncertaintyReason {
                    kind: UncertaintyKind::ParsePartial,
                    specifier: None,
                    line: None,
                }),
                ParseStatus::Failed => reasons.push(UncertaintyReason {
                    kind: UncertaintyKind::ParseFailed,
                    specifier: None,
                    line: None,
                }),
                ParseStatus::Ok => {}
            }
            if let Some(list) = unresolved_by.get(&v) {
                for u in list {
                    reasons.push(UncertaintyReason {
                        kind: UncertaintyKind::UnresolvedImport,
                        specifier: Some(u.specifier.clone()),
                        line: Some(u.line),
                    });
                }
            }
            if !reasons.is_empty() {
                uncertain.push(UncertainNode {
                    path: node.path.clone(),
                    category: node.category,
                    reasons,
                });
            }
        }
    }

    // 7. Warnings and summary.
    if depth_limited {
        warnings.push(Warning {
            code: WarningCode::ImpactDepthLimited,
            message: format!(
                "impact traversal stopped at max depth {}; more dependents exist beyond it",
                max_depth.unwrap_or(0)
            ),
        });
    }
    if !uncertain.is_empty() {
        warnings.push(Warning {
            code: WarningCode::ImpactUncertainty,
            message: format!(
                "{} file(s) not reached by the graph have unresolved imports or incomplete parses \
                 and may depend on changed code; see impact.uncertain",
                uncertain.len()
            ),
        });
    }

    let direct = affected
        .iter()
        .filter(|a| a.classification == ImpactClass::Direct)
        .count();
    let summary = ImpactSummary {
        changed_files: seeds.len(),
        affected: affected.len(),
        direct,
        transitive: affected.len() - direct,
        affected_tests: affected
            .iter()
            .filter(|a| a.category == FileCategory::Test)
            .count(),
        uncertain: uncertain.len(),
    };
    let changed: Vec<ChangedNode> = seeds
        .iter()
        .zip(counts)
        .map(|(s, (d, t))| ChangedNode {
            path: s.label.clone(),
            status: s.status,
            category: s.category,
            kind: s.kind,
            direct_dependents: d,
            transitive_dependents: t,
        })
        .collect();

    (
        ImpactReport {
            max_depth,
            depth_limited,
            ordering: ORDERING.to_string(),
            changed,
            affected,
            uncertain,
            ignored_changed_files: ignored,
            summary,
        },
        warnings,
    )
}

/// Path from the changed file to the affected node, plus the import statement on each step.
fn build_path(graph: &DependencyGraph, seed: &Seed, best: &Best) -> (Vec<String>, Vec<Hop>) {
    let mut path = Vec::new();
    let mut hops = Vec::new();

    if seed.node.is_none() {
        // Removed path: first hop is the (unresolved) import of that path.
        path.push(seed.label.clone());
        if let Some(&first) = best.chain.first() {
            if let Some(d) = seed.dangling.get(&first) {
                hops.push(Hop {
                    importer: graph.node(first).path.clone(),
                    imported: seed.label.clone(),
                    kind: d.kind,
                    type_only: d.type_only,
                    specifier: d.specifier.clone(),
                    line: d.line,
                });
            }
        }
    }
    path.extend(best.chain.iter().map(|&c| graph.node(c).path.clone()));
    for pair in best.chain.windows(2) {
        let (imported, importer) = (pair[0], pair[1]);
        if let Some(e) = graph.edge_between(importer, imported) {
            hops.push(Hop {
                importer: e.from.clone(),
                imported: e.to.clone(),
                kind: e.kind,
                type_only: e.type_only,
                specifier: e.specifier.clone(),
                line: e.line,
            });
        }
    }
    (path, hops)
}

fn describe(h: &Hop) -> String {
    let marker = if h.type_only { " [type-only]" } else { "" };
    format!(
        "{} imports \"{}\" at line {}{}",
        h.importer, h.specifier, h.line, marker
    )
}

fn explain(seed: &Seed, hops: &[Hop], distance: usize) -> String {
    let origin = match seed.kind {
        ChangedKind::Present => format!("changed file {}", seed.label),
        ChangedKind::Deleted => format!("deleted file {}", seed.label),
        ChangedKind::RenamedFrom => format!("{} (renamed away in this change)", seed.label),
    };
    // Describe from the affected file back toward the change.
    let chain = hops.iter().rev().map(describe).collect::<Vec<_>>().join("; ");
    if distance == 1 {
        format!("Direct dependent of {origin}: {chain}.")
    } else {
        format!("Transitive dependent of {origin} at distance {distance}: {chain}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dependency_graph::{DependencyEdge, GraphNode};

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
            specifier: format!("./{}", to.trim_end_matches(".ts")),
            line: 1,
        }
    }

    fn unresolved(from: &str, specifier: &str, reason: UnresolvedReason) -> UnresolvedImport {
        UnresolvedImport {
            from: from.to_string(),
            specifier: specifier.to_string(),
            reason,
            kind: ImportKind::Import,
            type_only: false,
            line: 3,
        }
    }

    /// `names` must already be in sorted order. Edges are `(importer, imported)`.
    fn graph_with(
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

    fn graph(names: &[&str], edges: &[(&str, &str)]) -> DependencyGraph {
        graph_with(names, edges, vec![])
    }

    fn modified(path: &str) -> ChangedFile {
        ChangedFile::new(path.to_string(), None, ChangeStatus::Modified)
    }

    fn run(g: &DependencyGraph, changed: &[ChangedFile], depth: Option<usize>) -> ImpactReport {
        analyze_impact(g, changed, None, depth).0
    }

    fn summary(r: &ImpactReport) -> Vec<(&str, ImpactClass, usize)> {
        r.affected
            .iter()
            .map(|a| (a.path.as_str(), a.classification, a.distance))
            .collect()
    }

    #[test]
    fn direct_transitive_and_unrelated() {
        // c -> b -> a ; x -> y (unrelated)
        let g = graph(
            &["a.ts", "b.ts", "c.ts", "x.ts", "y.ts"],
            &[("b.ts", "a.ts"), ("c.ts", "b.ts"), ("x.ts", "y.ts")],
        );
        let r = run(&g, &[modified("a.ts")], None);
        assert_eq!(
            summary(&r),
            vec![
                ("b.ts", ImpactClass::Direct, 1),
                ("c.ts", ImpactClass::Transitive, 2)
            ]
        );
        let c = &r.affected[1];
        assert_eq!(c.path_from_changed, vec!["a.ts", "b.ts", "c.ts"]);
        assert_eq!(c.hops.len(), 2);
        assert_eq!((c.hops[0].importer.as_str(), c.hops[0].imported.as_str()), ("b.ts", "a.ts"));
        assert_eq!((c.hops[1].importer.as_str(), c.hops[1].imported.as_str()), ("c.ts", "b.ts"));
        assert!(c.explanation.starts_with("Transitive dependent of changed file a.ts at distance 2"));
        assert!(r.uncertain.is_empty());
        assert_eq!(r.changed[0].direct_dependents, 1);
        assert_eq!(r.changed[0].transitive_dependents, 1);
        assert_eq!(r.summary.affected, 2);
        assert_eq!(r.summary.direct, 1);
    }

    #[test]
    fn cycles_terminate_and_never_list_the_changed_file_as_affected() {
        // a <-> b ; c imports a
        let g = graph(
            &["a.ts", "b.ts", "c.ts"],
            &[("a.ts", "b.ts"), ("b.ts", "a.ts"), ("c.ts", "a.ts")],
        );
        let r = run(&g, &[modified("a.ts")], None);
        assert_eq!(
            summary(&r),
            vec![("b.ts", ImpactClass::Direct, 1), ("c.ts", ImpactClass::Direct, 1)]
        );
    }

    #[test]
    fn multiple_changes_are_deduplicated_and_ranked_by_changed_dependency_count() {
        // c imports a and b ; d imports c ; e imports a only
        let g = graph(
            &["a.ts", "b.ts", "c.ts", "d.ts", "e.ts"],
            &[("c.ts", "a.ts"), ("c.ts", "b.ts"), ("d.ts", "c.ts"), ("e.ts", "a.ts")],
        );
        let r = run(&g, &[modified("b.ts"), modified("a.ts")], None);
        // distance 1: c (2 changed deps) before e (1); then d at distance 2.
        assert_eq!(
            summary(&r),
            vec![
                ("c.ts", ImpactClass::Direct, 1),
                ("e.ts", ImpactClass::Direct, 1),
                ("d.ts", ImpactClass::Transitive, 2)
            ]
        );
        assert_eq!(r.affected[0].changed_dependency_count, 2);
        assert_eq!(r.affected[0].affected_by.len(), 2);
        assert_eq!(r.affected[2].changed_dependency_count, 2); // d reaches both through c
        assert_eq!(r.affected.iter().map(|a| a.rank).collect::<Vec<_>>(), vec![1, 2, 3]);
    }

    #[test]
    fn ties_are_broken_by_path() {
        let g = graph(&["a.ts", "b.ts", "c.ts"], &[("c.ts", "a.ts"), ("b.ts", "a.ts")]);
        let r = run(&g, &[modified("a.ts")], None);
        assert_eq!(r.affected[0].path, "b.ts");
        assert_eq!(r.affected[1].path, "c.ts");
    }

    #[test]
    fn a_changed_file_that_imports_another_changed_file_is_not_listed_as_affected() {
        let g = graph(&["a.ts", "b.ts", "c.ts"], &[("b.ts", "a.ts"), ("c.ts", "b.ts")]);
        let r = run(&g, &[modified("a.ts"), modified("b.ts")], None);
        assert_eq!(summary(&r), vec![("c.ts", ImpactClass::Direct, 1)]);
        assert_eq!(r.summary.changed_files, 2);
        // a's own dependents exclude the changed file b.
        assert_eq!(r.changed[0].direct_dependents, 0);
        assert_eq!(r.changed[0].transitive_dependents, 1);
    }

    #[test]
    fn max_depth_truncates_and_reports_it() {
        let g = graph(
            &["a.ts", "b.ts", "c.ts", "d.ts"],
            &[("b.ts", "a.ts"), ("c.ts", "b.ts"), ("d.ts", "c.ts")],
        );
        let (r1, w1) = analyze_impact(&g, &[modified("a.ts")], None, Some(1));
        assert_eq!(summary(&r1), vec![("b.ts", ImpactClass::Direct, 1)]);
        assert!(r1.depth_limited);
        assert!(w1.iter().any(|w| w.code == WarningCode::ImpactDepthLimited));

        let (r2, _) = analyze_impact(&g, &[modified("a.ts")], None, Some(2));
        assert_eq!(r2.affected.len(), 2);
        assert!(r2.depth_limited);

        let (r3, w3) = analyze_impact(&g, &[modified("a.ts")], None, Some(3));
        assert_eq!(r3.affected.len(), 3);
        assert!(!r3.depth_limited);
        assert!(w3.is_empty());

        let (r0, _) = analyze_impact(&g, &[modified("a.ts")], None, Some(0));
        assert!(r0.affected.is_empty());
        assert!(r0.depth_limited);
    }

    #[test]
    fn deleted_file_makes_unresolved_importers_direct_dependents() {
        // b imports ./a (now missing); c imports b; d has an unrelated unresolved import.
        let g = graph_with(
            &["b.ts", "c.ts", "d.ts"],
            &[("c.ts", "b.ts")],
            vec![
                unresolved("b.ts", "./a", UnresolvedReason::NotFound),
                unresolved("d.ts", "./zzz", UnresolvedReason::NotFound),
            ],
        );
        let deleted = ChangedFile::new("a.ts".into(), None, ChangeStatus::Deleted);
        let (r, warnings) = analyze_impact(&g, &[deleted], None, None);

        assert_eq!(
            summary(&r),
            vec![("b.ts", ImpactClass::Direct, 1), ("c.ts", ImpactClass::Transitive, 2)]
        );
        assert_eq!(r.changed[0].kind, ChangedKind::Deleted);
        assert_eq!(r.changed[0].direct_dependents, 1);
        assert_eq!(r.affected[0].path_from_changed, vec!["a.ts", "b.ts"]);
        assert!(r.affected[0].explanation.contains("deleted file a.ts"));
        assert!(r.affected[0].explanation.contains("\"./a\""));
        // d's unrelated unresolved import is uncertain; b's is explained and not repeated.
        assert_eq!(r.uncertain.len(), 1);
        assert_eq!(r.uncertain[0].path, "d.ts");
        assert!(warnings.iter().any(|w| w.code == WarningCode::ImpactUncertainty));
    }

    #[test]
    fn renamed_file_treats_old_path_as_removed_and_new_path_as_changed() {
        // new.ts exists; b still imports ./old (unresolved); c imports new.ts.
        let g = graph_with(
            &["b.ts", "c.ts", "new.ts"],
            &[("c.ts", "new.ts")],
            vec![unresolved("b.ts", "./old", UnresolvedReason::NotFound)],
        );
        let renamed = ChangedFile::new("new.ts".into(), Some("old.ts".into()), ChangeStatus::Renamed);
        let r = run(&g, &[renamed], None);
        assert_eq!(
            summary(&r),
            vec![("b.ts", ImpactClass::Direct, 1), ("c.ts", ImpactClass::Direct, 1)]
        );
        let kinds: Vec<_> = r.changed.iter().map(|c| (c.path.as_str(), c.kind)).collect();
        assert_eq!(
            kinds,
            vec![("new.ts", ChangedKind::Present), ("old.ts", ChangedKind::RenamedFrom)]
        );
        assert!(r.affected[0].explanation.contains("renamed away"));
    }

    #[test]
    fn uncertain_nodes_are_unreachable_files_with_hidden_dependency_risk() {
        let mut partial = node("p.ts");
        partial.parse_status = ParseStatus::Partial;
        let g = DependencyGraph::from_parts(
            vec![node("a.ts"), node("b.ts"), node("m.ts"), partial, node("u.ts")],
            vec![edge("b.ts", "a.ts")],
            vec![],
            vec![
                // Reachable file's own unresolved import: not listed as uncertain.
                unresolved("b.ts", "./nope", UnresolvedReason::NotFound),
                // Asset import cannot hide a code dependency.
                unresolved("m.ts", "./data.json", UnresolvedReason::UnsupportedFileType),
                unresolved("u.ts", "@x/missing", UnresolvedReason::NotFound),
            ],
        );
        let r = run(&g, &[modified("a.ts")], None);
        let paths: Vec<_> = r.uncertain.iter().map(|u| u.path.as_str()).collect();
        assert_eq!(paths, vec!["p.ts", "u.ts"]);
        assert_eq!(r.uncertain[0].reasons[0].kind, UncertaintyKind::ParsePartial);
        assert_eq!(r.uncertain[1].reasons[0].kind, UncertaintyKind::UnresolvedImport);
        assert_eq!(r.uncertain[1].reasons[0].specifier.as_deref(), Some("@x/missing"));
        // Uncertain files are never mixed into the confirmed results.
        assert_eq!(summary(&r), vec![("b.ts", ImpactClass::Direct, 1)]);
    }

    #[test]
    fn no_changes_means_no_uncertainty_noise() {
        let g = graph_with(
            &["a.ts"],
            &[],
            vec![unresolved("a.ts", "./nope", UnresolvedReason::NotFound)],
        );
        let (r, w) = analyze_impact(&g, &[], None, None);
        assert!(r.affected.is_empty() && r.uncertain.is_empty() && r.changed.is_empty());
        assert_eq!(r.summary.changed_files, 0);
        assert!(w.is_empty());
    }

    #[test]
    fn untouched_changed_files_are_reported_not_dropped() {
        let g = graph(&["a.ts"], &[]);
        let mut excluded = ChangedFile::new("dist/x.js".into(), None, ChangeStatus::Modified);
        excluded.excluded = true;
        let changed = vec![
            modified("README.md"),
            modified("package.json"),
            excluded,
            modified("ghost.ts"),
            modified("a.ts"),
        ];
        let (r, warnings) = analyze_impact(&g, &changed, None, None);
        let ignored: Vec<_> = r
            .ignored_changed_files
            .iter()
            .map(|i| (i.path.as_str(), i.reason))
            .collect();
        assert_eq!(
            ignored,
            vec![
                ("README.md", IgnoredReason::NonSourceFile),
                ("dist/x.js", IgnoredReason::Excluded),
                ("ghost.ts", IgnoredReason::NotInGraph),
                ("package.json", IgnoredReason::NonSourceFile),
            ]
        );
        assert_eq!(r.summary.changed_files, 1);
        assert!(warnings
            .iter()
            .any(|w| w.code == WarningCode::UnmodeledConfigChange && w.message.contains("package.json")));
    }

    #[test]
    fn type_only_edges_are_marked_in_explanations_and_test_files_are_counted() {
        let mut e = edge("a.test.ts", "a.ts");
        e.type_only = true;
        let g = DependencyGraph::from_parts(
            vec![node("a.test.ts"), node("a.ts")],
            vec![e],
            vec![],
            vec![],
        );
        let r = run(&g, &[modified("a.ts")], None);
        assert!(r.affected[0].explanation.contains("[type-only]"));
        assert_eq!(r.affected[0].category, FileCategory::Test);
        assert_eq!(r.summary.affected_tests, 1);
    }

    #[test]
    fn results_are_deterministic() {
        let g = graph(
            &["a.ts", "b.ts", "c.ts", "d.ts"],
            &[("b.ts", "a.ts"), ("c.ts", "a.ts"), ("d.ts", "b.ts"), ("d.ts", "c.ts")],
        );
        let r1 = run(&g, &[modified("a.ts")], None);
        let r2 = run(&g, &[modified("a.ts")], None);
        assert_eq!(r1, r2);
        // d is reachable via b and c at equal length; the chosen path is stable (via b).
        assert_eq!(r1.affected[2].path_from_changed, vec!["a.ts", "b.ts", "d.ts"]);
    }
}
