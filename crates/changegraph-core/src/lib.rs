//! ChangeGraph analysis engine.
//!
//! Phases 1-2 implement repository validation, Git change detection, source discovery,
//! Tree-sitter import/export extraction, module resolution, the dependency graph, and
//! (Phase 3) change-impact analysis. Phase 4 adds `test_mapping` (see `docs/architecture.md`).
//!
//! The engine has no dependency on the CLI or any frontend.

pub mod config;
pub mod dependency_graph;
pub mod discovery;
pub mod error;
pub mod git_diff;
pub mod impact;
pub mod parser;
pub mod report;
pub mod repository;
pub mod resolver;
pub mod tsconfig;

mod git;

use std::time::Instant;

pub use config::{AnalysisConfig, ComparisonMode};
pub use error::{ChangeGraphError, Result};
pub use report::Report;

use discovery::ExcludeSet;
use report::{
    ComparisonInfo, DiscoveryInfo, ExecutionInfo, RepositoryInfo, ToolInfo, Warning, WarningCode,
    SCHEMA_VERSION,
};
use repository::Repository;

/// Run the analysis (Phases 1-3) and produce a [`Report`].
pub fn analyze(config: &AnalysisConfig) -> Result<Report> {
    let started = Instant::now();
    config.validate()?;

    let repo = Repository::open(&config.repo_path)?;
    let excludes = ExcludeSet::new(&config.exclude, config.use_default_excludes);
    let mut warnings: Vec<Warning> = Vec::new();

    let (comparison, mut changed_files) = match &config.base {
        Some(base) => {
            let target_name = config.target.clone().unwrap_or_else(|| "HEAD".to_string());
            let base_commit = repo.resolve_revision(base)?;
            let target_commit = repo.resolve_revision(&target_name)?;
            let merge_base =
                repo.merge_base(base, &target_name, &base_commit, &target_commit)?;
            let changes = git_diff::diff_commits(&repo, &merge_base, &target_commit)?;

            if repo.has_uncommitted_changes()? {
                warnings.push(Warning {
                    code: WarningCode::DirtyWorkingTree,
                    message: "the working tree has uncommitted changes; they are NOT included \
                              in this merge-base comparison"
                        .to_string(),
                });
            }

            // The dependency graph is built from the files on disk. If the target is not the
            // checked-out commit, the graph does not describe the target revision.
            if target_commit != repo.head() {
                warnings.push(Warning {
                    code: WarningCode::GraphTargetMismatch,
                    message: format!(
                        "the dependency graph is built from the checked-out working tree (HEAD {}), \
                         not from target `{}` ({}); impact results may not match the target revision",
                        &repo.head()[..repo.head().len().min(7)],
                        target_name,
                        &target_commit[..target_commit.len().min(7)]
                    ),
                });
            }

            let info = ComparisonInfo {
                mode: ComparisonMode::MergeBase,
                base: Some(base.clone()),
                target: Some(target_name),
                base_commit: Some(base_commit),
                target_commit: Some(target_commit),
                merge_base_commit: Some(merge_base),
            };
            (info, changes)
        }
        None => {
            let changes = git_diff::diff_working_tree(&repo, repo.head())?;
            let info = ComparisonInfo {
                mode: ComparisonMode::WorkingTree,
                base: None,
                target: None,
                base_commit: None,
                target_commit: None,
                merge_base_commit: None,
            };
            (info, changes)
        }
    };

    for file in &mut changed_files {
        file.excluded = excludes.matches_str(&file.path);
    }

    let (files, walk_warnings) = discovery::discover_sources(repo.root(), &excludes);
    warnings.extend(walk_warnings);

    let (tsconfig, tsconfig_warnings) = tsconfig::load(repo.root());
    warnings.extend(tsconfig_warnings);

    let (graph, graph_warnings) =
        dependency_graph::build_graph(repo.root(), &files, tsconfig.as_ref());
    warnings.extend(graph_warnings);

    let (impact, impact_warnings) =
        impact::analyze_impact(&graph, &changed_files, tsconfig.as_ref(), config.max_depth);
    warnings.extend(impact_warnings);
    tracing::debug!(
        changed = changed_files.len(),
        discovered = files.len(),
        nodes = graph.len(),
        "analysis complete"
    );

    Ok(Report {
        schema_version: SCHEMA_VERSION.to_string(),
        tool: ToolInfo {
            name: "changegraph".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        repository: RepositoryInfo {
            root: repo.root().display().to_string(),
            head_commit: repo.head().to_string(),
        },
        comparison,
        changed_files,
        discovery: DiscoveryInfo::new(&files, excludes.patterns().to_vec()),
        graph: graph.into_report(),
        impact,
        warnings,
        execution: ExecutionInfo {
            duration_ms: started.elapsed().as_millis() as u64,
        },
    })
}
