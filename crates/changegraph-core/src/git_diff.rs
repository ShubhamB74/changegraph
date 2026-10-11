//! Changed-file detection via `git diff`.
//!
//! Two strategies, never mixed (see `docs/algorithms.md`):
//! * [`diff_commits`] — commit-to-commit (used with the merge base as `from`).
//! * [`diff_working_tree`] — uncommitted work relative to `HEAD`, plus untracked files.

use serde::{Deserialize, Serialize};

use crate::discovery::{classify, FileCategory};
use crate::error::{ChangeGraphError, Result};
use crate::git::run_git_checked;
use crate::repository::Repository;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChanged,
    Unmerged,
    Untracked,
}

impl ChangeStatus {
    /// Single-letter code in the style of `git status`.
    pub fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Copied => 'C',
            Self::TypeChanged => 'T',
            Self::Unmerged => 'U',
            Self::Untracked => '?',
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangedFile {
    /// Repository-relative path (the new path for renames/copies).
    pub path: String,
    /// Previous path for renames/copies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub status: ChangeStatus,
    pub category: FileCategory,
    /// True if the path matches an exclusion pattern. Excluded files are still listed
    /// so that nothing is silently hidden; later phases will not analyze them.
    pub excluded: bool,
}

impl ChangedFile {
    pub fn new(path: String, old_path: Option<String>, status: ChangeStatus) -> Self {
        let category = classify(&path);
        Self {
            path,
            old_path,
            status,
            category,
            excluded: false,
        }
    }
}

/// Files changed between two commits (`git diff from to`).
pub fn diff_commits(repo: &Repository, from: &str, to: &str) -> Result<Vec<ChangedFile>> {
    let raw = run_git_checked(
        repo.root(),
        &[
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-color",
            from,
            to,
            "--",
        ],
    )?;
    let mut changes = parse_name_status_z(&raw)?;
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

/// Uncommitted changes relative to `head` (staged + unstaged) plus untracked,
/// non-ignored files.
pub fn diff_working_tree(repo: &Repository, head: &str) -> Result<Vec<ChangedFile>> {
    let raw = run_git_checked(
        repo.root(),
        &[
            "diff",
            "--name-status",
            "-z",
            "-M",
            "--no-ext-diff",
            "--no-color",
            head,
            "--",
        ],
    )?;
    let mut changes = parse_name_status_z(&raw)?;

    let untracked = run_git_checked(
        repo.root(),
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    for token in untracked.split(|b| *b == 0).filter(|t| !t.is_empty()) {
        changes.push(ChangedFile::new(
            String::from_utf8_lossy(token).into_owned(),
            None,
            ChangeStatus::Untracked,
        ));
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(changes)
}

/// Parse the output of `git diff --name-status -z`.
///
/// Format: `STATUS NUL PATH NUL` for most entries and `Rnn NUL OLD NUL NEW NUL`
/// (likewise `C`) for renames and copies. Non-UTF-8 paths are converted lossily.
pub fn parse_name_status_z(raw: &[u8]) -> Result<Vec<ChangedFile>> {
    let mut tokens = raw.split(|b| *b == 0).filter(|t| !t.is_empty());
    let mut out = Vec::new();

    while let Some(code) = tokens.next() {
        let code = String::from_utf8_lossy(code).into_owned();
        let Some(letter) = code.chars().next() else {
            continue;
        };
        let status = match letter {
            'A' => ChangeStatus::Added,
            'M' => ChangeStatus::Modified,
            'D' => ChangeStatus::Deleted,
            'R' => ChangeStatus::Renamed,
            'C' => ChangeStatus::Copied,
            'T' => ChangeStatus::TypeChanged,
            'U' => ChangeStatus::Unmerged,
            _ => {
                return Err(ChangeGraphError::GitOutputParse(format!(
                    "unknown diff status `{code}`"
                )))
            }
        };
        let missing = || ChangeGraphError::GitOutputParse(format!("status `{code}` has no path"));
        let to_string = |t: &[u8]| String::from_utf8_lossy(t).into_owned();

        if matches!(status, ChangeStatus::Renamed | ChangeStatus::Copied) {
            let old = tokens.next().map(to_string).ok_or_else(missing)?;
            let new = tokens.next().map(to_string).ok_or_else(missing)?;
            out.push(ChangedFile::new(new, Some(old), status));
        } else {
            let path = tokens.next().map(to_string).ok_or_else(missing)?;
            out.push(ChangedFile::new(path, None, status));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_entries() {
        let raw = b"M\0src/a.ts\0A\0src/b.test.ts\0D\0README.md\0";
        let v = parse_name_status_z(raw).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v[0].status, ChangeStatus::Modified);
        assert_eq!(v[0].category, FileCategory::Source);
        assert_eq!(v[1].category, FileCategory::Test);
        assert_eq!(v[2].category, FileCategory::Other);
    }

    #[test]
    fn parses_renames_with_both_paths() {
        let raw = b"R087\0src/old.ts\0src/new.ts\0M\0x.ts\0";
        let v = parse_name_status_z(raw).unwrap();
        assert_eq!(v[0].status, ChangeStatus::Renamed);
        assert_eq!(v[0].old_path.as_deref(), Some("src/old.ts"));
        assert_eq!(v[0].path, "src/new.ts");
        assert_eq!(v[1].path, "x.ts");
    }

    #[test]
    fn handles_paths_with_spaces_and_unicode() {
        let raw = "M\0src/my file é.ts\0".as_bytes();
        let v = parse_name_status_z(raw).unwrap();
        assert_eq!(v[0].path, "src/my file é.ts");
    }

    #[test]
    fn empty_input_is_empty_list() {
        assert!(parse_name_status_z(b"").unwrap().is_empty());
    }

    #[test]
    fn truncated_or_unknown_input_is_an_error() {
        assert!(parse_name_status_z(b"M\0").is_err());
        assert!(parse_name_status_z(b"R100\0only-old\0").is_err());
        assert!(parse_name_status_z(b"Z\0a.ts\0").is_err());
    }
}
