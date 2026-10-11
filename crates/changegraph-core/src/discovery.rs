//! Source-file discovery, classification, and exclusion rules.
//!
//! Discovery is intentionally independent of Git: it walks the working directory,
//! honours `.gitignore` files *inside the tree* (not global/user-level ignore files, so
//! results are reproducible across machines), and applies [`ExcludeSet`].

use std::path::Path;

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};

use crate::report::{Warning, WarningCode};

/// Extensions treated as analyzable TypeScript/JavaScript sources.
pub const SOURCE_EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts"];

/// Directory/file names excluded unless `use_default_excludes` is disabled.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "out",
    "coverage",
    ".next",
    ".turbo",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileCategory {
    Source,
    Test,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFile {
    /// Repository-relative path using `/` separators.
    pub path: String,
    pub category: FileCategory,
}

/// Exclusion patterns.
///
/// A pattern without `/` matches any path component (e.g. `dist`).
/// A pattern containing `/` matches that relative path or anything beneath it.
/// Globs are not supported yet.
#[derive(Debug, Clone)]
pub struct ExcludeSet {
    patterns: Vec<String>,
}

impl ExcludeSet {
    pub fn new(user: &[String], use_defaults: bool) -> Self {
        let mut patterns: Vec<String> = Vec::new();
        if use_defaults {
            patterns.extend(DEFAULT_EXCLUDES.iter().map(|s| s.to_string()));
        }
        patterns.extend(user.iter().filter_map(|p| normalize_pattern(p)));
        patterns.sort();
        patterns.dedup();
        Self { patterns }
    }

    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }

    pub fn matches(&self, rel: &Path) -> bool {
        self.matches_str(&path_to_string(rel))
    }

    /// `rel` must use `/` separators and be repository-relative.
    pub fn matches_str(&self, rel: &str) -> bool {
        self.patterns.iter().any(|pat| {
            if pat.contains('/') {
                rel == pat || (rel.starts_with(pat.as_str()) && rel[pat.len()..].starts_with('/'))
            } else {
                rel.split('/').any(|component| component == pat)
            }
        })
    }
}

fn normalize_pattern(raw: &str) -> Option<String> {
    let p = raw.trim().replace('\\', "/");
    let p = p.trim_start_matches("./").trim_end_matches('/');
    if p.is_empty() {
        None
    } else {
        Some(p.to_string())
    }
}

/// Convert a relative path to a `/`-separated string (lossy for non-UTF-8 names).
pub fn path_to_string(rel: &Path) -> String {
    rel.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Filename-based test heuristic: `*.test.*`, `*.spec.*`, or any `__tests__` directory.
/// This is a heuristic, not proof that a file contains tests.
pub fn is_test_path(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
    stem.ends_with(".test") || stem.ends_with(".spec") || path.split('/').any(|c| c == "__tests__")
}

pub fn has_source_extension(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    file_name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .is_some_and(|ext| SOURCE_EXTENSIONS.contains(&ext.as_str()))
}

pub fn classify(path: &str) -> FileCategory {
    if !has_source_extension(path) {
        FileCategory::Other
    } else if is_test_path(path) {
        FileCategory::Test
    } else {
        FileCategory::Source
    }
}

/// Walk `root` and return all analyzable files (sorted by path) plus any walk warnings.
pub fn discover_sources(root: &Path, excludes: &ExcludeSet) -> (Vec<SourceFile>, Vec<Warning>) {
    let mut warnings = Vec::new();
    let mut files = Vec::new();

    let root_owned = root.to_path_buf();
    let filter_excludes = excludes.clone();

    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .follow_links(false)
        .parents(false)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .filter_entry(move |entry| match entry.path().strip_prefix(&root_owned) {
            Ok(rel) if !rel.as_os_str().is_empty() => !filter_excludes.matches(rel),
            _ => true,
        });

    for result in builder.build() {
        match result {
            Ok(entry) => {
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    continue;
                }
                let Ok(rel) = entry.path().strip_prefix(root) else {
                    continue;
                };
                let rel = path_to_string(rel);
                let category = classify(&rel);
                if category != FileCategory::Other {
                    files.push(SourceFile {
                        path: rel,
                        category,
                    });
                }
            }
            Err(err) => warnings.push(Warning {
                code: WarningCode::WalkError,
                message: err.to_string(),
            }),
        }
    }

    files.sort_by(|a, b| a.path.cmp(&b.path));
    (files, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_files() {
        assert_eq!(classify("src/a.ts"), FileCategory::Source);
        assert_eq!(classify("src/a.test.ts"), FileCategory::Test);
        assert_eq!(classify("src/a.spec.jsx"), FileCategory::Test);
        assert_eq!(classify("src/__tests__/a.js"), FileCategory::Test);
        assert_eq!(classify("README.md"), FileCategory::Other);
        assert_eq!(classify("src/a.TS"), FileCategory::Source);
        assert_eq!(classify("Makefile"), FileCategory::Other);
    }

    #[test]
    fn exclude_name_matches_any_component() {
        let ex = ExcludeSet::new(&[], true);
        assert!(ex.matches_str("node_modules/x/index.js"));
        assert!(ex.matches_str("packages/a/dist/index.js"));
        assert!(!ex.matches_str("src/distribution/a.ts"));
    }

    #[test]
    fn exclude_path_prefix_matches_boundary_only() {
        let ex = ExcludeSet::new(&["src/legacy/".to_string()], false);
        assert!(ex.matches_str("src/legacy"));
        assert!(ex.matches_str("src/legacy/a.ts"));
        assert!(!ex.matches_str("src/legacy-new/a.ts"));
    }

    #[test]
    fn defaults_can_be_disabled_and_blank_patterns_ignored() {
        let ex = ExcludeSet::new(&["  ".to_string(), "./".to_string()], false);
        assert!(ex.patterns().is_empty());
        assert!(!ex.matches_str("node_modules/a.js"));
    }
}
