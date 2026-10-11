//! Thin wrapper around the `git` CLI. Nothing else in the crate spawns processes.

use std::path::Path;
use std::process::{Command, Output};

use crate::error::{ChangeGraphError, Result};

/// Run `git -C <cwd> <args...>` and return the raw output regardless of exit status.
pub(crate) fn run_git(cwd: &Path, args: &[&str]) -> Result<Output> {
    tracing::debug!(?cwd, ?args, "running git");
    Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .map_err(ChangeGraphError::GitUnavailable)
}

/// Like [`run_git`] but converts a non-zero exit into [`ChangeGraphError::GitCommandFailed`]
/// and returns stdout bytes.
pub(crate) fn run_git_checked(cwd: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = run_git(cwd, args)?;
    if !out.status.success() {
        return Err(ChangeGraphError::GitCommandFailed {
            command: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(out.stdout)
}
