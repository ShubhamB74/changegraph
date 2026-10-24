//! Versioned report schema and writers.
//!
//! The JSON schema is documented in `docs/report-schema.md` and
//! `docs/schema/report-0.4.0.schema.json`. While the major version is 0 the schema may
//! change between releases; `schema_version` always reflects the emitted shape.
//!
//! Determinism: for identical repository state and configuration every field is
//! identical **except** `execution.duration_ms` (wall-clock time), which is `null` when timing
//! is disabled (`--no-timing`), making the whole report byte-identical.

use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::config::ComparisonMode;
use crate::dependency_graph::GraphReport;
use crate::discovery::{FileCategory, SourceFile};
use crate::error::{ChangeGraphError, Result};
use crate::git_diff::ChangedFile;
use crate::impact::ImpactReport;
use crate::test_mapping::TestReport;

pub const SCHEMA_VERSION: &str = "0.4.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WarningCode {
    /// A filesystem error occurred while walking the tree.
    WalkError,
    /// Merge-base mode was used but the working tree has uncommitted changes
    /// that are NOT part of the comparison.
    DirtyWorkingTree,
    /// A file has syntax errors (partial extraction) or could not be parsed at all.
    ParseError,
    /// A discovered file could not be read.
    ReadError,
    /// `tsconfig.json` is malformed or uses unsupported features (e.g. `extends`).
    TsconfigIssue,
    /// The dependency graph was built from the checked-out working tree, which is not the
    /// target revision of the comparison.
    GraphTargetMismatch,
    /// Impact traversal stopped at `max_depth` while more dependents existed.
    ImpactDepthLimited,
    /// Unaffected files have unresolved imports or incomplete parses that may hide dependents.
    ImpactUncertainty,
    /// A changed configuration/dependency file may affect many files in ways the graph
    /// does not model.
    UnmodeledConfigChange,
    /// The recommended test list may be missing tests that should run.
    TestSelectionIncomplete,
    /// The optional coverage map referenced tests that are not analyzed files.
    CoverageMapIssue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub code: WarningCode,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryInfo {
    /// Absolute path of the repository root.
    pub root: String,
    /// Full SHA of `HEAD`.
    pub head_commit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonInfo {
    pub mode: ComparisonMode,
    /// Revision names as given by the user (merge-base mode only).
    pub base: Option<String>,
    pub target: Option<String>,
    /// Resolved commit SHAs (merge-base mode only).
    pub base_commit: Option<String>,
    pub target_commit: Option<String>,
    pub merge_base_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryInfo {
    pub total_files: usize,
    pub source_files: usize,
    pub test_files: usize,
    /// Effective exclusion patterns (sorted).
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionInfo {
    /// Wall-clock time; the only non-deterministic field. `None` when timing is disabled.
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: String,
    pub tool: ToolInfo,
    pub repository: RepositoryInfo,
    pub comparison: ComparisonInfo,
    pub changed_files: Vec<ChangedFile>,
    pub discovery: DiscoveryInfo,
    pub graph: GraphReport,
    pub impact: ImpactReport,
    pub test_recommendations: TestReport,
    pub warnings: Vec<Warning>,
    pub execution: ExecutionInfo,
}

impl DiscoveryInfo {
    pub fn new(files: &[SourceFile], exclude_patterns: Vec<String>) -> Self {
        let test_files = files
            .iter()
            .filter(|f| f.category == FileCategory::Test)
            .count();
        Self {
            total_files: files.len(),
            source_files: files.len() - test_files,
            test_files,
            exclude_patterns,
        }
    }
}

/// Stream the report as pretty JSON without building an intermediate `String`.
pub fn write_json<W: Write>(report: &Report, writer: W) -> Result<()> {
    write_json_with(report, writer, true)
}

/// Stream the report as JSON (`pretty = false` omits whitespace, which shrinks large reports).
/// The report is serialized directly into `writer`; no intermediate copy is built.
pub fn write_json_with<W: Write>(report: &Report, mut writer: W, pretty: bool) -> Result<()> {
    if pretty {
        serde_json::to_writer_pretty(&mut writer, report)?;
    } else {
        serde_json::to_writer(&mut writer, report)?;
    }
    writer.write_all(b"\n").map_err(ChangeGraphError::Output)?;
    writer.flush().map_err(ChangeGraphError::Output)
}

/// Human-readable terminal rendering.
pub fn write_text<W: Write>(report: &Report, mut writer: W) -> Result<()> {
    render_text(report, &mut writer).map_err(ChangeGraphError::Output)
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

fn render_text<W: Write>(r: &Report, w: &mut W) -> std::io::Result<()> {
    writeln!(w, "ChangeGraph {} (schema {})", r.tool.version, r.schema_version)?;
    writeln!(w, "Repository: {}", r.repository.root)?;

    let c = &r.comparison;
    match c.mode {
        ComparisonMode::MergeBase => writeln!(
            w,
            "Comparison: committed changes on {} ({}) since merge base with {} ({}) [merge base {}]",
            c.target.as_deref().unwrap_or("HEAD"),
            short(c.target_commit.as_deref().unwrap_or("")),
            c.base.as_deref().unwrap_or("?"),
            short(c.base_commit.as_deref().unwrap_or("")),
            short(c.merge_base_commit.as_deref().unwrap_or("")),
        )?,
        ComparisonMode::WorkingTree => writeln!(
            w,
            "Comparison: uncommitted changes (staged, unstaged, untracked) vs HEAD ({})",
            short(&r.repository.head_commit)
        )?,
    }

    writeln!(w)?;
    writeln!(w, "Changed files ({}):", r.changed_files.len())?;
    if r.changed_files.is_empty() {
        writeln!(w, "  (none)")?;
    }
    for f in &r.changed_files {
        let category = match f.category {
            FileCategory::Source => "source",
            FileCategory::Test => "test",
            FileCategory::Other => "other",
        };
        let excluded = if f.excluded { ", excluded" } else { "" };
        match &f.old_path {
            Some(old) => writeln!(
                w,
                "  {}  {} -> {}  [{category}{excluded}]",
                f.status.letter(),
                old,
                f.path
            )?,
            None => writeln!(w, "  {}  {}  [{category}{excluded}]", f.status.letter(), f.path)?,
        }
    }

    writeln!(w)?;
    writeln!(
        w,
        "Discovered {} analyzable files ({} source, {} test)",
        r.discovery.total_files, r.discovery.source_files, r.discovery.test_files
    )?;

    let g = &r.graph;
    let st = &g.stats;
    writeln!(w)?;
    writeln!(w, "Dependency graph ({}):", crate::dependency_graph::EDGE_SEMANTICS)?;
    writeln!(
        w,
        "  {} files, {} internal edges, {} external imports, {} unresolved imports",
        st.files, st.edges, st.external_dependencies, st.unresolved_imports
    )?;
    let pct = if st.files == 0 {
        100.0
    } else {
        100.0 * st.parsed_ok as f64 / st.files as f64
    };
    writeln!(
        w,
        "  parsed cleanly: {}/{} ({:.1}%), partial: {}, failed: {}",
        st.parsed_ok, st.files, pct, st.parsed_partial, st.parsed_failed
    )?;
    if !g.cycles.is_empty() {
        writeln!(w, "  import cycles ({}):", g.cycles.len())?;
        for cycle in &g.cycles {
            writeln!(w, "    {}", cycle.join(" <-> "))?;
        }
    }
    const MAX_UNRESOLVED_SHOWN: usize = 20;
    if !g.unresolved_imports.is_empty() {
        writeln!(w, "  unresolved imports (not treated as dependencies):")?;
        for u in g.unresolved_imports.iter().take(MAX_UNRESOLVED_SHOWN) {
            writeln!(w, "    {}:{}  \"{}\"  [{:?}]", u.from, u.line, u.specifier, u.reason)?;
        }
        if g.unresolved_imports.len() > MAX_UNRESOLVED_SHOWN {
            writeln!(
                w,
                "    ... and {} more (use --format json for the full list)",
                g.unresolved_imports.len() - MAX_UNRESOLVED_SHOWN
            )?;
        }
    }

    render_impact(r, w)?;
    render_tests(r, w)?;

    if !r.warnings.is_empty() {
        writeln!(w)?;
        writeln!(w, "Warnings ({}):", r.warnings.len())?;
        for warning in &r.warnings {
            writeln!(w, "  [{:?}] {}", warning.code, warning.message)?;
        }
    }

    writeln!(w)?;
    match r.execution.duration_ms {
        Some(ms) => writeln!(w, "Completed in {ms} ms"),
        None => Ok(()),
    }
}

fn render_impact<W: Write>(r: &Report, w: &mut W) -> std::io::Result<()> {
    const MAX_SHOWN: usize = 25;
    let im = &r.impact;
    let s = &im.summary;
    writeln!(w)?;
    writeln!(w, "Impact (reverse dependency traversal; ordering: {}):", im.ordering)?;
    if s.changed_files == 0 {
        writeln!(w, "  no analyzable changed files")?;
    } else {
        writeln!(
            w,
            "  {} changed -> {} affected ({} direct, {} transitive; {} are test files)",
            s.changed_files, s.affected, s.direct, s.transitive, s.affected_tests
        )?;
    }
    if let Some(depth) = im.max_depth {
        let note = if im.depth_limited { " -- TRUNCATED, more dependents exist" } else { "" };
        writeln!(w, "  depth limit: {depth}{note}")?;
    }
    for item in im.affected.iter().take(MAX_SHOWN) {
        writeln!(
            w,
            "  {:>3}. {} [{}, distance {}, {} changed dep(s)]",
            item.rank,
            item.path,
            match item.classification {
                crate::impact::ImpactClass::Direct => "direct",
                crate::impact::ImpactClass::Transitive => "transitive",
            },
            item.distance,
            item.changed_dependency_count
        )?;
        writeln!(w, "       {}", item.explanation)?;
    }
    if im.affected.len() > MAX_SHOWN {
        writeln!(
            w,
            "  ... and {} more (use --format json for the full list)",
            im.affected.len() - MAX_SHOWN
        )?;
    }
    if !im.uncertain.is_empty() {
        writeln!(
            w,
            "  uncertain ({}): unaffected per the graph, but unresolved imports or incomplete parses may hide a dependency:",
            im.uncertain.len()
        )?;
        for u in im.uncertain.iter().take(MAX_SHOWN) {
            writeln!(w, "    {}", u.path)?;
        }
        if im.uncertain.len() > MAX_SHOWN {
            writeln!(w, "    ... and {} more", im.uncertain.len() - MAX_SHOWN)?;
        }
    }
    if !im.ignored_changed_files.is_empty() {
        writeln!(
            w,
            "  changed files not analyzed ({}): non-source, excluded, or not in the graph",
            im.ignored_changed_files.len()
        )?;
    }
    Ok(())
}

fn render_tests<W: Write>(r: &Report, w: &mut W) -> std::io::Result<()> {
    use crate::test_mapping::Basis;
    const MAX_SHOWN: usize = 25;
    let t = &r.test_recommendations;
    let s = &t.summary;
    writeln!(w)?;
    writeln!(
        w,
        "Recommended tests ({} of {} test files; {} on a known basis, {} heuristic only):",
        s.recommended, s.total_test_files, s.known_basis, s.heuristic_only
    )?;
    if let Some(cm) = &t.coverage_map {
        writeln!(w, "  coverage map: {} ({} tests)", cm.path, cm.tests)?;
    }
    if t.recommendations.is_empty() {
        writeln!(w, "  (none identified)")?;
    }
    for rec in t.recommendations.iter().take(MAX_SHOWN) {
        let basis = match rec.basis {
            Basis::Known => "known",
            Basis::Heuristic => "HEURISTIC",
        };
        writeln!(w, "  {:>3}. {} [{}]", rec.rank, rec.path, basis)?;
        writeln!(w, "       {}", rec.explanation)?;
    }
    if t.recommendations.len() > MAX_SHOWN {
        writeln!(
            w,
            "  ... and {} more (use --format json for the full list)",
            t.recommendations.len() - MAX_SHOWN
        )?;
    }
    if !t.unmapped_modules.is_empty() {
        writeln!(
            w,
            "  impacted modules with NO test linked by a known method ({}):",
            t.unmapped_modules.len()
        )?;
        for u in t.unmapped_modules.iter().take(MAX_SHOWN) {
            if u.heuristic_tests.is_empty() {
                writeln!(w, "    {}", u.path)?;
            } else {
                writeln!(w, "    {}  (name-match guess only: {})", u.path, u.heuristic_tests.join(", "))?;
            }
        }
        if t.unmapped_modules.len() > MAX_SHOWN {
            writeln!(w, "    ... and {} more", t.unmapped_modules.len() - MAX_SHOWN)?;
        }
    }
    if t.completeness.complete {
        writeln!(
            w,
            "  no known gaps in test selection (this is still not a guarantee that these tests catch every regression)"
        )?;
    } else {
        writeln!(w, "  TEST SELECTION IS INCOMPLETE -- do not treat this list as the full set to run:")?;
        for reason in &t.completeness.reasons {
            writeln!(w, "    - {}", reason.detail)?;
        }
    }
    Ok(())
}
