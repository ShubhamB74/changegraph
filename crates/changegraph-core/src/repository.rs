//! Repository validation and revision resolution.

use std::io;
use std::path::{Path, PathBuf};

use crate::error::{ChangeGraphError, Result};
use crate::git::{run_git, run_git_checked};

/// A validated Git working tree.
///
/// If the supplied path is a subdirectory of a repository, the *repository root*
/// (`git rev-parse --show-toplevel`) is used: Git reports diff paths relative to it.
#[derive(Debug, Clone)]
pub struct Repository {
    root: PathBuf,
    head: String,
}

impl Repository {
    pub fn open(path: &Path) -> Result<Self> {
        let meta = std::fs::metadata(path).map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                ChangeGraphError::PathNotFound(path.to_path_buf())
            } else {
                ChangeGraphError::Io {
                    path: path.to_path_buf(),
                    source,
                }
            }
        })?;
        if !meta.is_dir() {
            return Err(ChangeGraphError::NotADirectory(path.to_path_buf()));
        }
        let canonical = path
            .canonicalize()
            .map_err(|source| ChangeGraphError::Io {
                path: path.to_path_buf(),
                source,
            })?;

        let out = run_git(&canonical, &["rev-parse", "--show-toplevel"])?;
        if !out.status.success() {
            return Err(ChangeGraphError::NotAGitRepository(canonical));
        }
        let top = String::from_utf8_lossy(&out.stdout)
            .trim_end_matches(['\n', '\r'])
            .to_string();

        let mut repo = Self {
            root: PathBuf::from(top),
            head: String::new(),
        };
        repo.head = repo.resolve_revision("HEAD")?;
        Ok(repo)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Full commit SHA of `HEAD`.
    pub fn head(&self) -> &str {
        &self.head
    }

    /// Resolve `revision` to a full commit SHA, or fail with an actionable error.
    pub fn resolve_revision(&self, revision: &str) -> Result<String> {
        if revision.is_empty() {
            return Err(ChangeGraphError::InvalidRevision {
                revision: revision.to_string(),
                reason: "revision is empty",
            });
        }
        if revision.starts_with('-') {
            return Err(ChangeGraphError::InvalidRevision {
                revision: revision.to_string(),
                reason: "revisions must not start with '-'",
            });
        }
        let spec = format!("{revision}^{{commit}}");
        let out = run_git(&self.root, &["rev-parse", "--verify", "--quiet", &spec])?;
        if !out.status.success() {
            return Err(ChangeGraphError::RevisionNotFound {
                revision: revision.to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Best common ancestor of two commits. `base_name`/`target_name` are only used in errors.
    pub fn merge_base(
        &self,
        base_name: &str,
        target_name: &str,
        base_commit: &str,
        target_commit: &str,
    ) -> Result<String> {
        let out = run_git(&self.root, &["merge-base", base_commit, target_commit])?;
        match out.status.code() {
            Some(0) => Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()),
            Some(1) => Err(ChangeGraphError::NoMergeBase {
                base: base_name.to_string(),
                target: target_name.to_string(),
            }),
            _ => Err(ChangeGraphError::GitCommandFailed {
                command: "merge-base".to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            }),
        }
    }

    /// True if the working tree has staged, unstaged, or untracked changes.
    pub fn has_uncommitted_changes(&self) -> Result<bool> {
        let out = run_git_checked(&self.root, &["status", "--porcelain=v1", "-z"])?;
        Ok(!out.is_empty())
    }
}
