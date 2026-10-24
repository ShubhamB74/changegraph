//! File-level dependency graph.
//!
//! **Edge semantics: `A -> B` means "A imports B"** (A depends on B). A change to B can
//! therefore affect A, so downstream *impact* is computed by walking edges in reverse
//! (see [`DependencyGraph::reverse_reachable`]). Both adjacency directions are stored so
//! reverse traversal is O(out-degree) per node.
//!
//! Only *confirmed internal* relationships become edges. External packages and imports that
//! could not be resolved are recorded separately and never treated as edges.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::discovery::{FileCategory, SourceFile};
use crate::parser::{ImportKind, Language, ParseStatus, ParsedFile, SourceParser};
use crate::report::{Warning, WarningCode};
use crate::resolver::{ModuleResolver, Resolution, UnresolvedReason};
use crate::tsconfig::TsConfig;

pub const EDGE_SEMANTICS: &str = "A -> B means module A imports module B";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    pub path: String,
    pub category: FileCategory,
    pub parse_status: ParseStatus,
    /// Names of local exports (best effort; see `parser` docs).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<String>,
}

/// One resolved import statement. Several edges may connect the same pair of files
/// (e.g. an import plus a re-export); each remains inspectable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub from: String,
    pub to: String,
    pub kind: ImportKind,
    pub type_only: bool,
    pub specifier: String,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalDependency {
    pub from: String,
    pub package: String,
    pub specifier: String,
    pub kind: ImportKind,
    pub type_only: bool,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnresolvedImport {
    pub from: String,
    pub specifier: String,
    pub reason: UnresolvedReason,
    pub kind: ImportKind,
    pub type_only: bool,
    pub line: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphStats {
    pub files: usize,
    pub edges: usize,
    pub external_dependencies: usize,
    pub unresolved_imports: usize,
    pub parsed_ok: usize,
    pub parsed_partial: usize,
    pub parsed_failed: usize,
    pub cycle_count: usize,
}

/// Serializable form embedded in the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphReport {
    pub edge_semantics: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<DependencyEdge>,
    pub external_dependencies: Vec<ExternalDependency>,
    pub unresolved_imports: Vec<UnresolvedImport>,
    /// Groups of files that import each other (strongly connected components of size > 1,
    /// plus self-importing files), each sorted by path.
    pub cycles: Vec<Vec<String>>,
    pub stats: GraphStats,
}

/// In-memory graph with adjacency for traversal. Node indices follow the (sorted) order of
/// `nodes`, which makes every traversal result deterministic.
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    nodes: Vec<GraphNode>,
    index: HashMap<String, usize>,
    edges: Vec<DependencyEdge>,
    /// `forward[a]` = sorted, deduplicated files that `a` imports.
    forward: Vec<Vec<usize>>,
    /// `reverse[b]` = sorted, deduplicated files that import `b`.
    reverse: Vec<Vec<usize>>,
    external: Vec<ExternalDependency>,
    unresolved: Vec<UnresolvedImport>,
    /// `(importer, imported)` -> index of the first (lowest-line) edge between the pair.
    edge_index: HashMap<(usize, usize), usize>,
}

impl DependencyGraph {
    /// Build from parts. Edges referencing unknown paths are dropped.
    pub fn from_parts(
        nodes: Vec<GraphNode>,
        edges: Vec<DependencyEdge>,
        external: Vec<ExternalDependency>,
        unresolved: Vec<UnresolvedImport>,
    ) -> Self {
        let index: HashMap<String, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.path.clone(), i))
            .collect();
        let edges: Vec<DependencyEdge> = edges
            .into_iter()
            .filter(|e| index.contains_key(&e.from) && index.contains_key(&e.to))
            .collect();

        let mut forward = vec![Vec::new(); nodes.len()];
        let mut reverse = vec![Vec::new(); nodes.len()];
        let mut edge_index: HashMap<(usize, usize), usize> = HashMap::new();
        for (i, e) in edges.iter().enumerate() {
            let (a, b) = (index[&e.from], index[&e.to]);
            forward[a].push(b);
            reverse[b].push(a);
            edge_index.entry((a, b)).or_insert(i);
        }
        for list in forward.iter_mut().chain(reverse.iter_mut()) {
            list.sort_unstable();
            list.dedup();
        }
        Self {
            nodes,
            index,
            edges,
            forward,
            reverse,
            external,
            unresolved,
            edge_index,
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node_index(&self, path: &str) -> Option<usize> {
        self.index.get(path).copied()
    }

    pub fn node(&self, index: usize) -> &GraphNode {
        &self.nodes[index]
    }

    pub fn nodes(&self) -> &[GraphNode] {
        &self.nodes
    }

    pub fn unresolved_imports(&self) -> &[UnresolvedImport] {
        &self.unresolved
    }

    /// A representative edge for "`importer` imports `imported`" (the earliest by line).
    pub fn edge_between(&self, importer: usize, imported: usize) -> Option<&DependencyEdge> {
        self.edge_index
            .get(&(importer, imported))
            .map(|&i| &self.edges[i])
    }

    /// Files that `node` imports.
    pub fn dependencies(&self, node: usize) -> &[usize] {
        &self.forward[node]
    }

    /// Files that import `node` (its direct dependents).
    pub fn dependents(&self, node: usize) -> &[usize] {
        &self.reverse[node]
    }

    /// Breadth-first walk over *reverse* edges from `starts`.
    ///
    /// Returns `(node, distance)` for every node reachable from a start (excluding the starts
    /// themselves), sorted by `(distance, node)`. Distance is the length of the shortest
    /// dependent chain. Cycles are safe: each node is visited at most once.
    /// `max_depth = Some(n)` stops expanding at distance `n`. Out-of-range starts are ignored.
    pub fn reverse_reachable(
        &self,
        starts: &[usize],
        max_depth: Option<usize>,
    ) -> Vec<(usize, usize)> {
        let n = self.nodes.len();
        let mut dist = vec![usize::MAX; n];
        let mut queue = VecDeque::new();
        for &s in starts {
            if s < n && dist[s] == usize::MAX {
                dist[s] = 0;
                queue.push_back(s);
            }
        }
        let mut out = Vec::new();
        while let Some(v) = queue.pop_front() {
            let d = dist[v];
            if max_depth.is_some_and(|m| d >= m) {
                continue;
            }
            for &w in &self.reverse[v] {
                if dist[w] == usize::MAX {
                    dist[w] = d + 1;
                    out.push((w, d + 1));
                    queue.push_back(w);
                }
            }
        }
        out.sort_unstable_by_key(|&(node, d)| (d, node));
        out
    }

    /// Import cycles: strongly connected components with more than one node, plus nodes that
    /// import themselves. Iterative Tarjan; each group and the list are sorted.
    #[allow(clippy::needless_range_loop)] // classic index-based Tarjan; clearer than iterators
    pub fn cycles(&self) -> Vec<Vec<usize>> {
        let n = self.nodes.len();
        let mut index = vec![usize::MAX; n];
        let mut low = vec![0usize; n];
        let mut on_stack = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();
        let mut next_index = 0usize;
        let mut result: Vec<Vec<usize>> = Vec::new();

        for root in 0..n {
            if index[root] != usize::MAX {
                continue;
            }
            index[root] = next_index;
            low[root] = next_index;
            next_index += 1;
            stack.push(root);
            on_stack[root] = true;
            // (node, position of next child to visit)
            let mut call: Vec<(usize, usize)> = vec![(root, 0)];

            while let Some(&(v, child_pos)) = call.last() {
                if child_pos < self.forward[v].len() {
                    if let Some(top) = call.last_mut() {
                        top.1 += 1;
                    }
                    let w = self.forward[v][child_pos];
                    if index[w] == usize::MAX {
                        index[w] = next_index;
                        low[w] = next_index;
                        next_index += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        call.push((w, 0));
                    } else if on_stack[w] {
                        low[v] = low[v].min(index[w]);
                    }
                } else {
                    call.pop();
                    if let Some(&(parent, _)) = call.last() {
                        low[parent] = low[parent].min(low[v]);
                    }
                    if low[v] == index[v] {
                        let mut component = Vec::new();
                        while let Some(w) = stack.pop() {
                            on_stack[w] = false;
                            component.push(w);
                            if w == v {
                                break;
                            }
                        }
                        if component.len() > 1 || self.forward[v].contains(&v) {
                            component.sort_unstable();
                            result.push(component);
                        }
                    }
                }
            }
        }
        result.sort();
        result
    }

    /// Consume the graph and produce the serializable report section.
    pub fn into_report(self) -> GraphReport {
        let cycles: Vec<Vec<String>> = self
            .cycles()
            .into_iter()
            .map(|group| {
                group
                    .into_iter()
                    .map(|i| self.nodes[i].path.clone())
                    .collect()
            })
            .collect();

        let count = |s: ParseStatus| self.nodes.iter().filter(|n| n.parse_status == s).count();
        let stats = GraphStats {
            files: self.nodes.len(),
            edges: self.edges.len(),
            external_dependencies: self.external.len(),
            unresolved_imports: self.unresolved.len(),
            parsed_ok: count(ParseStatus::Ok),
            parsed_partial: count(ParseStatus::Partial),
            parsed_failed: count(ParseStatus::Failed),
            cycle_count: cycles.len(),
        };
        GraphReport {
            edge_semantics: EDGE_SEMANTICS.to_string(),
            nodes: self.nodes,
            edges: self.edges,
            external_dependencies: self.external,
            unresolved_imports: self.unresolved,
            cycles,
            stats,
        }
    }
}

/// Parse every file, resolve its imports, and assemble the graph.
///
/// `files` must be sorted by path (as returned by `discover_sources`). Read failures and
/// syntax errors become warnings; they never abort the analysis.
pub fn build_graph(
    root: &Path,
    files: &[SourceFile],
    tsconfig: Option<&TsConfig>,
) -> (DependencyGraph, Vec<Warning>) {
    let known: HashSet<String> = files.iter().map(|f| f.path.clone()).collect();
    let resolver = ModuleResolver::new(&known, tsconfig);
    let mut parser = SourceParser::new();
    let mut warnings = Vec::new();

    let mut nodes = Vec::with_capacity(files.len());
    let mut edges = Vec::new();
    let mut external = Vec::new();
    let mut unresolved = Vec::new();

    for file in files {
        let parsed = parse_file(root, file, &mut parser, &mut warnings);

        for imp in &parsed.imports {
            match resolver.resolve(&file.path, &imp.specifier) {
                Resolution::Internal(to) => edges.push(DependencyEdge {
                    from: file.path.clone(),
                    to,
                    kind: imp.kind,
                    type_only: imp.type_only,
                    specifier: imp.specifier.clone(),
                    line: imp.line,
                }),
                Resolution::External { package } => external.push(ExternalDependency {
                    from: file.path.clone(),
                    package,
                    specifier: imp.specifier.clone(),
                    kind: imp.kind,
                    type_only: imp.type_only,
                    line: imp.line,
                }),
                Resolution::Unresolved(reason) => unresolved.push(UnresolvedImport {
                    from: file.path.clone(),
                    specifier: imp.specifier.clone(),
                    reason,
                    kind: imp.kind,
                    type_only: imp.type_only,
                    line: imp.line,
                }),
            }
        }

        nodes.push(GraphNode {
            path: file.path.clone(),
            category: file.category,
            parse_status: parsed.status,
            exports: parsed.exports,
        });
    }

    (
        DependencyGraph::from_parts(nodes, edges, external, unresolved),
        warnings,
    )
}

fn parse_file(
    root: &Path,
    file: &SourceFile,
    parser: &mut SourceParser,
    warnings: &mut Vec<Warning>,
) -> ParsedFile {
    let Some(language) = Language::from_path(&file.path) else {
        warnings.push(Warning {
            code: WarningCode::ParseError,
            message: format!("{}: no grammar for this file type; skipped", file.path),
        });
        return ParsedFile::failed();
    };
    let bytes = match std::fs::read(root.join(&file.path)) {
        Ok(b) => b,
        Err(e) => {
            warnings.push(Warning {
                code: WarningCode::ReadError,
                message: format!("{}: could not read file: {e}", file.path),
            });
            return ParsedFile::failed();
        }
    };
    let parsed = parser.parse(language, &bytes);
    match parsed.status {
        ParseStatus::Ok => {}
        ParseStatus::Partial => warnings.push(Warning {
            code: WarningCode::ParseError,
            message: format!(
                "{}: syntax errors{}; imports/exports were extracted only from statements that parsed",
                file.path,
                parsed
                    .first_error_line
                    .map(|l| format!(" (first near line {l})"))
                    .unwrap_or_default()
            ),
        }),
        ParseStatus::Failed => warnings.push(Warning {
            code: WarningCode::ParseError,
            message: format!("{}: could not be parsed; no dependencies extracted", file.path),
        }),
    }
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(path: &str) -> GraphNode {
        GraphNode {
            path: path.to_string(),
            category: FileCategory::Source,
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
            specifier: format!("./{to}"),
            line: 1,
        }
    }

    /// Nodes are named so that sorted order == declaration order.
    fn graph(names: &[&str], edges: &[(&str, &str)]) -> DependencyGraph {
        DependencyGraph::from_parts(
            names.iter().map(|n| node(n)).collect(),
            edges.iter().map(|(a, b)| edge(a, b)).collect(),
            vec![],
            vec![],
        )
    }

    fn names(g: &DependencyGraph, list: &[(usize, usize)]) -> Vec<(String, usize)> {
        list.iter()
            .map(|&(i, d)| (g.node(i).path.clone(), d))
            .collect()
    }

    #[test]
    fn forward_and_reverse_adjacency_follow_import_direction() {
        // a imports b; c imports b.
        let g = graph(&["a", "b", "c"], &[("a", "b"), ("c", "b")]);
        let (a, b, c) = (0, 1, 2);
        assert_eq!(g.dependencies(a), &[b]);
        assert_eq!(g.dependents(b), &[a, c]);
        assert!(g.dependents(a).is_empty());
    }

    #[test]
    fn duplicate_edges_between_a_pair_keep_all_edges_but_one_adjacency_entry() {
        let g = graph(&["a", "b"], &[("a", "b"), ("a", "b")]);
        assert_eq!(g.dependencies(0), &[1]);
        assert_eq!(g.dependents(1), &[0]);
        assert_eq!(g.into_report().edges.len(), 2);
    }

    #[test]
    fn reverse_reachable_covers_direct_transitive_and_skips_unrelated() {
        // d -> c -> b -> a ; x -> y (unrelated)
        let g = graph(
            &["a", "b", "c", "d", "x", "y"],
            &[("b", "a"), ("c", "b"), ("d", "c"), ("x", "y")],
        );
        let a = g.node_index("a").unwrap();
        let out = names(&g, &g.reverse_reachable(&[a], None));
        assert_eq!(
            out,
            vec![
                ("b".to_string(), 1),
                ("c".to_string(), 2),
                ("d".to_string(), 3)
            ]
        );
    }

    #[test]
    fn reverse_reachable_respects_max_depth() {
        let g = graph(&["a", "b", "c", "d"], &[("b", "a"), ("c", "b"), ("d", "c")]);
        let out = names(&g, &g.reverse_reachable(&[0], Some(2)));
        assert_eq!(out, vec![("b".to_string(), 1), ("c".to_string(), 2)]);
        assert!(g.reverse_reachable(&[0], Some(0)).is_empty());
    }

    #[test]
    fn reverse_reachable_terminates_on_cycles_and_uses_shortest_distance() {
        // a <-> b cycle, c imports b, d imports a and c.
        let g = graph(
            &["a", "b", "c", "d"],
            &[("a", "b"), ("b", "a"), ("c", "b"), ("d", "a"), ("d", "c")],
        );
        let out = names(&g, &g.reverse_reachable(&[0], None));
        // Starts are excluded even though `a` is reachable from itself via the cycle.
        assert_eq!(
            out,
            vec![
                ("b".to_string(), 1),
                ("d".to_string(), 1),
                ("c".to_string(), 2)
            ]
        );
    }

    #[test]
    fn multiple_starts_are_deduplicated_and_ordered_deterministically() {
        let g = graph(&["a", "b", "c"], &[("c", "a"), ("c", "b")]);
        let out = names(&g, &g.reverse_reachable(&[1, 0, 1], None));
        assert_eq!(out, vec![("c".to_string(), 1)]);
    }

    #[test]
    fn detects_cycles_and_self_loops_but_not_acyclic_chains() {
        let g = graph(
            &["a", "b", "c", "d", "e", "f"],
            &[("a", "b"), ("b", "c"), ("c", "a"), ("d", "d"), ("e", "f")],
        );
        assert_eq!(g.cycles(), vec![vec![0, 1, 2], vec![3]]);
    }

    #[test]
    fn cycle_detection_handles_nested_components_and_deep_chains() {
        // Two separate 2-cycles joined by a one-way edge, plus a 5_000-node chain
        // (iterative algorithm must not overflow the stack).
        let mut names_owned: Vec<String> =
            ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let mut edges: Vec<(String, String)> = vec![
            ("a".into(), "b".into()),
            ("b".into(), "a".into()),
            ("b".into(), "c".into()),
            ("c".into(), "d".into()),
            ("d".into(), "c".into()),
        ];
        for i in 0..5_000 {
            names_owned.push(format!("z{i:05}"));
        }
        for i in 1..5_000 {
            edges.push((format!("z{i:05}"), format!("z{:05}", i - 1)));
        }
        let name_refs: Vec<&str> = names_owned.iter().map(String::as_str).collect();
        let edge_refs: Vec<(&str, &str)> = edges
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let g = graph(&name_refs, &edge_refs);
        assert_eq!(g.cycles(), vec![vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn edges_to_unknown_nodes_are_dropped() {
        let g = graph(&["a"], &[("a", "ghost")]);
        assert!(g.dependencies(0).is_empty());
        assert!(g.into_report().edges.is_empty());
    }

    #[test]
    fn report_stats_and_cycles_are_consistent() {
        let g = graph(&["a", "b"], &[("a", "b"), ("b", "a")]);
        let r = g.into_report();
        assert_eq!(r.stats.files, 2);
        assert_eq!(r.stats.edges, 2);
        assert_eq!(r.stats.cycle_count, 1);
        assert_eq!(r.stats.parsed_ok, 2);
        assert_eq!(r.cycles, vec![vec!["a".to_string(), "b".to_string()]]);
        assert_eq!(r.edge_semantics, EDGE_SEMANTICS);
    }
}
