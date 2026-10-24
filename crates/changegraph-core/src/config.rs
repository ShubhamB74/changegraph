//! Analysis configuration and comparison modes.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{ChangeGraphError, Result};

/// How "what changed" is defined. The modes are never mixed within one report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonMode {
    /// Committed changes on `target` since its merge base with `base`
    /// (the semantics of `git diff base...target`). Uncommitted work is excluded
    /// and a warning is emitted if any exists.
    MergeBase,
    /// Uncommitted changes (staged + unstaged, plus untracked files) relative to `HEAD`.
    /// Used when no `--base` is given.
    WorkingTree,
}

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub repo_path: PathBuf,
    /// Base revision. `None` selects [`ComparisonMode::WorkingTree`].
    pub base: Option<String>,
    /// Target revision; defaults to `HEAD`. Only valid together with `base`.
    pub target: Option<String>,
    /// Extra exclusions (directory/file name, or `/`-separated relative path prefix).
    pub exclude: Vec<String>,
    /// Apply the built-in exclusion list (`node_modules`, `.git`, `dist`, ...).
    pub use_default_excludes: bool,
    /// Maximum dependent distance to follow during impact analysis (`None` = unlimited).
    pub max_depth: Option<usize>,
}

impl AnalysisConfig {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
            base: None,
            target: None,
            exclude: Vec::new(),
            use_default_excludes: true,
            max_depth: None,
        }
    }

    pub fn mode(&self) -> ComparisonMode {
        if self.base.is_some() {
            ComparisonMode::MergeBase
        } else {
            ComparisonMode::WorkingTree
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.target.is_some() && self.base.is_none() {
            return Err(ChangeGraphError::InvalidConfig(
                "a target revision requires a base revision".into(),
            ));
        }
        Ok(())
    }
}
