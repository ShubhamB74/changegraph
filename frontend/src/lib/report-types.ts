// Types mirroring docs/schema/report-0.4.0.schema.json.
// Runtime validation lives in validate-report.ts; these types are only trusted after it passes.

export const SUPPORTED_SCHEMA_VERSION = "0.4.0";

export type FileCategory = "source" | "test" | "other";
export type ParseStatus = "ok" | "partial" | "failed";
export type ImportKind = "import" | "re_export" | "re_export_all";
export type ChangeStatus =
  | "added"
  | "modified"
  | "deleted"
  | "renamed"
  | "copied"
  | "type_changed"
  | "unmerged"
  | "untracked";
export type UnresolvedReason =
  | "not_found"
  | "outside_repository"
  | "unsupported_file_type"
  | "absolute_path";
export type WarningCode =
  | "walk_error"
  | "dirty_working_tree"
  | "parse_error"
  | "read_error"
  | "tsconfig_issue"
  | "graph_target_mismatch"
  | "impact_depth_limited"
  | "impact_uncertainty"
  | "unmodeled_config_change"
  | "test_selection_incomplete"
  | "coverage_map_issue";
export type TestMethod =
  | "changed_test"
  | "direct_import"
  | "dependency_reachability"
  | "coverage_map"
  | "filename_heuristic";
export type ModuleStatus = "changed" | "affected" | "removed";
export type IncompleteKind =
  | "unmapped_modules"
  | "depth_limited"
  | "uncertain_files"
  | "unmodeled_config_change"
  | "unanalyzed_changed_files"
  | "graph_target_mismatch"
  | "no_tests_recommended";

export interface Warning {
  code: WarningCode;
  message: string;
}

export interface ChangedFile {
  path: string;
  old_path?: string;
  status: ChangeStatus;
  category: FileCategory;
  excluded: boolean;
}

export interface GraphNode {
  path: string;
  category: "source" | "test";
  parse_status: ParseStatus;
  exports?: string[];
}

export interface DependencyEdge {
  from: string;
  to: string;
  kind: ImportKind;
  type_only: boolean;
  specifier: string;
  line: number;
}

export interface ExternalDependency {
  from: string;
  package: string;
  specifier: string;
  kind: ImportKind;
  type_only: boolean;
  line: number;
}

export interface UnresolvedImport {
  from: string;
  specifier: string;
  reason: UnresolvedReason;
  kind: ImportKind;
  type_only: boolean;
  line: number;
}

export interface GraphStats {
  files: number;
  edges: number;
  external_dependencies: number;
  unresolved_imports: number;
  parsed_ok: number;
  parsed_partial: number;
  parsed_failed: number;
  cycle_count: number;
}

export interface GraphSection {
  edge_semantics: string;
  nodes: GraphNode[];
  edges: DependencyEdge[];
  external_dependencies: ExternalDependency[];
  unresolved_imports: UnresolvedImport[];
  cycles: string[][];
  stats: GraphStats;
}

export interface ChangedNode {
  path: string;
  status: ChangeStatus;
  category: FileCategory;
  kind: "present" | "deleted" | "renamed_from";
  direct_dependents: number;
  transitive_dependents: number;
}

export interface Hop {
  importer: string;
  imported: string;
  kind: ImportKind;
  type_only: boolean;
  specifier: string;
  line: number;
}

export interface AffectedBy {
  changed_file: string;
  distance: number;
}

export interface AffectedNode {
  rank: number;
  path: string;
  category: FileCategory;
  classification: "direct" | "transitive";
  distance: number;
  changed_dependency_count: number;
  affected_by: AffectedBy[];
  path_from_changed: string[];
  hops: Hop[];
  explanation: string;
}

export interface UncertaintyReason {
  kind: "unresolved_import" | "parse_partial" | "parse_failed";
  specifier?: string;
  line?: number;
}

export interface UncertainNode {
  path: string;
  category: FileCategory;
  reasons: UncertaintyReason[];
}

export interface ImpactSection {
  max_depth: number | null;
  depth_limited: boolean;
  ordering: string;
  changed: ChangedNode[];
  affected: AffectedNode[];
  uncertain: UncertainNode[];
  ignored_changed_files: { path: string; reason: "non_source_file" | "excluded" | "not_in_graph" }[];
  summary: {
    changed_files: number;
    affected: number;
    direct: number;
    transitive: number;
    affected_tests: number;
    uncertain: number;
  };
}

export interface Coverage {
  module: string;
  module_status: ModuleStatus;
  method: TestMethod;
  distance: number | null;
  via?: string;
  detail: string;
}

export interface TestRecommendation {
  rank: number;
  path: string;
  basis: "known" | "heuristic";
  methods: TestMethod[];
  min_distance: number | null;
  covers: Coverage[];
  explanation: string;
}

export interface TestSection {
  coverage_map: { path: string; tests: number } | null;
  ordering: string;
  recommendations: TestRecommendation[];
  unmapped_modules: { path: string; status: ModuleStatus; heuristic_tests: string[] }[];
  completeness: {
    complete: boolean;
    reasons: { kind: IncompleteKind; detail: string }[];
  };
  summary: {
    total_test_files: number;
    recommended: number;
    known_basis: number;
    heuristic_only: number;
  };
}

export interface Report {
  schema_version: string;
  tool: { name: string; version: string };
  repository: { root: string; head_commit: string };
  comparison: {
    mode: "merge_base" | "working_tree";
    base: string | null;
    target: string | null;
    base_commit: string | null;
    target_commit: string | null;
    merge_base_commit: string | null;
  };
  changed_files: ChangedFile[];
  discovery: {
    total_files: number;
    source_files: number;
    test_files: number;
    exclude_patterns: string[];
  };
  graph: GraphSection;
  impact: ImpactSection;
  test_recommendations: TestSection;
  warnings: Warning[];
  execution: { duration_ms: number | null };
}
