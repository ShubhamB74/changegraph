//! Structured, actionable errors.
//!
//! Every variant maps to a stable process exit code (see [`ChangeGraphError::exit_code`])
//! and, where useful, a one-line remediation hint shown by the CLI.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ChangeGraphError>;

#[derive(Debug, Error)]
pub enum ChangeGraphError {
    #[error("path does not exist: {}", .0.display())]
    PathNotFound(PathBuf),

    #[error("path is not a directory: {}", .0.display())]
    NotADirectory(PathBuf),

    #[error("not inside a Git repository: {}", .0.display())]
    NotAGitRepository(PathBuf),

    #[error("could not execute `git`: {0}")]
    GitUnavailable(#[source] io::Error),

    #[error("revision `{revision}` could not be resolved to a commit")]
    RevisionNotFound { revision: String },

    #[error("invalid revision `{revision}`: {reason}")]
    InvalidRevision {
        revision: String,
        reason: &'static str,
    },

    #[error("`{base}` and `{target}` have no common ancestor, so no merge base exists")]
    NoMergeBase { base: String, target: String },

    #[error("`git {command}` failed: {stderr}")]
    GitCommandFailed { command: String, stderr: String },

    #[error("unexpected output from git: {0}")]
    GitOutputParse(String),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("I/O error on {}: {source}", .path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("failed to write output: {0}")]
    Output(#[source] io::Error),

    #[error("failed to serialize report: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl ChangeGraphError {
    /// Documented process exit code for this error.
    ///
    /// | code | meaning                                          |
    /// |------|--------------------------------------------------|
    /// | 1    | internal / I/O / serialization failure           |
    /// | 2    | invalid usage or configuration                   |
    /// | 3    | path or repository problem                       |
    /// | 4    | revision problem (unknown, invalid, no merge base) |
    /// | 5    | `git` missing or a git command failed            |
    pub fn exit_code(&self) -> u8 {
        use ChangeGraphError::*;
        match self {
            Io { .. } | Output(_) | Serialize(_) => 1,
            InvalidConfig(_) => 2,
            PathNotFound(_) | NotADirectory(_) | NotAGitRepository(_) => 3,
            RevisionNotFound { .. } | InvalidRevision { .. } | NoMergeBase { .. } => 4,
            GitUnavailable(_) | GitCommandFailed { .. } | GitOutputParse(_) => 5,
        }
    }

    /// A short remediation hint, if one applies.
    pub fn hint(&self) -> Option<&'static str> {
        use ChangeGraphError::*;
        match self {
            PathNotFound(_) => Some("check the path you passed to `analyze`"),
            NotADirectory(_) => Some("pass the repository directory, not a file"),
            NotAGitRepository(_) => {
                Some("run inside a Git working tree (`git init` or `git clone`)")
            }
            GitUnavailable(_) => Some("install Git and make sure `git` is on your PATH"),
            RevisionNotFound { .. } => Some(
                "check spelling with `git rev-parse --verify <rev>`; \
                 in a repo with no commits, create one first; \
                 in CI, make sure the base branch was fetched (e.g. `fetch-depth: 0`)",
            ),
            NoMergeBase { .. } => {
                Some("the histories are unrelated or the checkout is shallow; fetch full history")
            }
            _ => None,
        }
    }
}
