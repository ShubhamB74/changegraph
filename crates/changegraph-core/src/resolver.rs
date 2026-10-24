//! Module-specifier resolution against the set of analyzed files.
//!
//! Resolution order for a specifier imported from file `F`:
//!
//! 1. `node:` prefix → external (Node built-in).
//! 2. Leading `/` → unresolved (`absolute_path`; unsupported).
//! 3. Relative (`./`, `../`, `.`, `..`) → join with `F`'s directory, normalize, then *probe*.
//! 4. Otherwise, if `tsconfig.json` defines `paths`, the longest matching pattern wins; its
//!    targets are probed in order. A matched alias that finds no file is `unresolved`
//!    (it is never silently demoted to an external package).
//! 5. Otherwise, if `baseUrl` is set, probe the specifier against it.
//! 6. Otherwise → external package.
//!
//! *Probe* order for a normalized path `P`:
//!
//! 1. TypeScript ESM-style extension mapping: `P.js → P.ts, P.tsx`; `.jsx → .tsx`;
//!    `.mjs → .mts`; `.cjs → .cts`.
//! 2. `P` exactly.
//! 3. `P` + `.ts .tsx .d.ts .mts .cts .js .jsx .mjs .cjs`.
//! 4. `P/index` + the same extensions.
//!
//! Only files in the analyzed set can be targets. A relative import into an excluded
//! directory therefore appears as `unresolved`, never as a confirmed edge.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::tsconfig::TsConfig;

const PROBE_EXTENSIONS: &[&str] = &["ts", "tsx", "d.ts", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// Extensions of files we never analyze; an import of one is reported as such rather than
/// as a generic miss.
const ASSET_EXTENSIONS: &[&str] = &[
    "json", "css", "scss", "sass", "less", "svg", "png", "jpg", "jpeg", "gif", "webp", "ico", "md",
    "html", "txt", "wasm", "graphql", "gql", "yaml", "yml", "woff", "woff2", "mp4", "mp3",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// A file in the analyzed set (repository-relative path).
    Internal(String),
    /// A package or built-in outside the repository.
    External {
        package: String,
    },
    Unresolved(UnresolvedReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnresolvedReason {
    /// No analyzed file matches (it may not exist, or may be in an excluded directory).
    NotFound,
    /// The path escapes the repository root.
    OutsideRepository,
    /// Imports a non-source asset (`.json`, `.css`, ...) which is not analyzed.
    UnsupportedFileType,
    /// Absolute filesystem path.
    AbsolutePath,
}

pub struct ModuleResolver<'a> {
    files: &'a HashSet<String>,
    tsconfig: Option<&'a TsConfig>,
}

impl<'a> ModuleResolver<'a> {
    pub fn new(files: &'a HashSet<String>, tsconfig: Option<&'a TsConfig>) -> Self {
        Self { files, tsconfig }
    }

    pub fn resolve(&self, importer: &str, specifier: &str) -> Resolution {
        if specifier.starts_with("node:") {
            return Resolution::External {
                package: specifier.to_string(),
            };
        }
        if specifier.starts_with('/') {
            return Resolution::Unresolved(UnresolvedReason::AbsolutePath);
        }
        if is_relative(specifier) {
            let dir = importer.rsplit_once('/').map_or("", |(d, _)| d);
            return match normalize_join(dir, specifier) {
                None => Resolution::Unresolved(UnresolvedReason::OutsideRepository),
                Some(path) => match self.probe(&path) {
                    Some(found) => Resolution::Internal(found),
                    None => Resolution::Unresolved(miss_reason(specifier)),
                },
            };
        }
        if let Some(cfg) = self.tsconfig {
            if let Some(resolution) = self.resolve_with_tsconfig(cfg, specifier) {
                return resolution;
            }
        }
        Resolution::External {
            package: package_name(specifier),
        }
    }

    fn resolve_with_tsconfig(&self, cfg: &TsConfig, specifier: &str) -> Option<Resolution> {
        // Longest-prefix alias match; an exact (wildcard-free) pattern always wins.
        let mut best: Option<(usize, &crate::tsconfig::PathAlias, String)> = None;
        for alias in &cfg.aliases {
            let (specificity, captured) = match alias.pattern.split_once('*') {
                Some((prefix, suffix)) => {
                    if specifier.len() >= prefix.len() + suffix.len()
                        && specifier.starts_with(prefix)
                        && specifier.ends_with(suffix)
                    {
                        let mid = &specifier[prefix.len()..specifier.len() - suffix.len()];
                        (prefix.len(), mid.to_string())
                    } else {
                        continue;
                    }
                }
                None => {
                    if alias.pattern == specifier {
                        (usize::MAX, String::new())
                    } else {
                        continue;
                    }
                }
            };
            if best.as_ref().is_none_or(|(s, _, _)| specificity > *s) {
                best = Some((specificity, alias, captured));
            }
        }

        if let Some((_, alias, captured)) = best {
            for target in &alias.targets {
                let target = target.replace('*', &captured);
                if let Some(path) = normalize_join(&cfg.base_dir, &target) {
                    if let Some(found) = self.probe(&path) {
                        return Some(Resolution::Internal(found));
                    }
                }
            }
            return Some(Resolution::Unresolved(UnresolvedReason::NotFound));
        }

        if cfg.has_base_url {
            if let Some(path) = normalize_join(&cfg.base_dir, specifier) {
                if let Some(found) = self.probe(&path) {
                    return Some(Resolution::Internal(found));
                }
            }
        }
        None
    }

    /// Probe a normalized repository-relative path (see module docs for the order).
    fn probe(&self, path: &str) -> Option<String> {
        const MAPPED: &[(&str, &[&str])] = &[
            (".js", &["ts", "tsx"]),
            (".jsx", &["tsx"]),
            (".mjs", &["mts"]),
            (".cjs", &["cts"]),
        ];
        for (js_ext, ts_exts) in MAPPED {
            if let Some(stem) = path.strip_suffix(js_ext) {
                for ext in *ts_exts {
                    let candidate = format!("{stem}.{ext}");
                    if self.files.contains(&candidate) {
                        return Some(candidate);
                    }
                }
            }
        }
        if self.files.contains(path) {
            return Some(path.to_string());
        }
        for ext in PROBE_EXTENSIONS {
            let candidate = format!("{path}.{ext}");
            if self.files.contains(&candidate) {
                return Some(candidate);
            }
        }
        for ext in PROBE_EXTENSIONS {
            let candidate = if path.is_empty() {
                format!("index.{ext}")
            } else {
                format!("{path}/index.{ext}")
            };
            if self.files.contains(&candidate) {
                return Some(candidate);
            }
        }
        None
    }
}

fn is_relative(specifier: &str) -> bool {
    specifier == "."
        || specifier == ".."
        || specifier.starts_with("./")
        || specifier.starts_with("../")
}

fn miss_reason(specifier: &str) -> UnresolvedReason {
    let last = specifier.rsplit('/').next().unwrap_or(specifier);
    match last.rsplit_once('.') {
        Some((_, ext)) if ASSET_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) => {
            UnresolvedReason::UnsupportedFileType
        }
        _ => UnresolvedReason::NotFound,
    }
}

/// `@scope/name/deep/path` → `@scope/name`; `name/deep` → `name`.
pub fn package_name(specifier: &str) -> String {
    let mut parts = specifier.split('/');
    match parts.next() {
        Some(first) if first.starts_with('@') => match parts.next() {
            Some(second) => format!("{first}/{second}"),
            None => first.to_string(),
        },
        Some(first) => first.to_string(),
        None => specifier.to_string(),
    }
}

/// Join `rel` onto the already-normalized directory `dir`, resolving `.` and `..`.
/// Returns `None` if the result would escape the repository root.
pub fn normalize_join(dir: &str, rel: &str) -> Option<String> {
    let mut parts: Vec<&str> = if dir.is_empty() {
        Vec::new()
    } else {
        dir.split('/').collect()
    };
    for component in rel.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tsconfig::{PathAlias, TsConfig};

    fn files(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn internal(p: &str) -> Resolution {
        Resolution::Internal(p.to_string())
    }

    #[test]
    fn normalize_join_handles_dots_and_escapes() {
        assert_eq!(
            normalize_join("src/a", "../b/c").as_deref(),
            Some("src/b/c")
        );
        assert_eq!(normalize_join("", "./x").as_deref(), Some("x"));
        assert_eq!(normalize_join("src", "..").as_deref(), Some(""));
        assert_eq!(normalize_join("src", "../.."), None);
    }

    #[test]
    fn package_names() {
        assert_eq!(package_name("react"), "react");
        assert_eq!(package_name("lodash/fp"), "lodash");
        assert_eq!(package_name("@scope/pkg/deep"), "@scope/pkg");
        assert_eq!(package_name("@scope"), "@scope");
    }

    #[test]
    fn relative_resolution_extensions_and_index() {
        let set = files(&["src/a.ts", "src/b/index.tsx", "src/c.js", "src/d.d.ts"]);
        let r = ModuleResolver::new(&set, None);
        assert_eq!(r.resolve("src/x.ts", "./a"), internal("src/a.ts"));
        assert_eq!(r.resolve("src/x.ts", "./b"), internal("src/b/index.tsx"));
        assert_eq!(r.resolve("src/x.ts", "./c"), internal("src/c.js"));
        assert_eq!(r.resolve("src/x.ts", "./d"), internal("src/d.d.ts"));
        assert_eq!(r.resolve("src/sub/y.ts", "../a.ts"), internal("src/a.ts"));
    }

    #[test]
    fn typescript_style_js_extension_maps_to_ts() {
        let set = files(&["src/a.ts", "src/b.tsx", "src/c.js"]);
        let r = ModuleResolver::new(&set, None);
        assert_eq!(r.resolve("src/x.ts", "./a.js"), internal("src/a.ts"));
        assert_eq!(r.resolve("src/x.ts", "./b.jsx"), internal("src/b.tsx"));
        // A real .js file is still found.
        assert_eq!(r.resolve("src/x.ts", "./c.js"), internal("src/c.js"));
    }

    #[test]
    fn misses_are_unresolved_with_reasons() {
        let set = files(&["src/a.ts"]);
        let r = ModuleResolver::new(&set, None);
        assert_eq!(
            r.resolve("src/x.ts", "./nope"),
            Resolution::Unresolved(UnresolvedReason::NotFound)
        );
        assert_eq!(
            r.resolve("src/x.ts", "./data.json"),
            Resolution::Unresolved(UnresolvedReason::UnsupportedFileType)
        );
        assert_eq!(
            r.resolve("src/x.ts", "../../outside"),
            Resolution::Unresolved(UnresolvedReason::OutsideRepository)
        );
        assert_eq!(
            r.resolve("src/x.ts", "/abs/path"),
            Resolution::Unresolved(UnresolvedReason::AbsolutePath)
        );
        // Dotted names that are not assets are ordinary misses.
        assert_eq!(
            r.resolve("src/x.ts", "./user.service"),
            Resolution::Unresolved(UnresolvedReason::NotFound)
        );
    }

    #[test]
    fn bare_specifiers_are_external_not_internal() {
        let set = files(&["src/react.ts"]);
        let r = ModuleResolver::new(&set, None);
        assert_eq!(
            r.resolve("src/x.ts", "react"),
            Resolution::External {
                package: "react".into()
            }
        );
        assert_eq!(
            r.resolve("src/x.ts", "node:fs"),
            Resolution::External {
                package: "node:fs".into()
            }
        );
    }

    fn cfg() -> TsConfig {
        TsConfig {
            base_dir: "src".into(),
            has_base_url: true,
            aliases: vec![
                PathAlias {
                    pattern: "@lib/*".into(),
                    targets: vec!["lib/*".into()],
                },
                PathAlias {
                    pattern: "@lib/special/*".into(),
                    targets: vec!["special/*".into()],
                },
                PathAlias {
                    pattern: "config".into(),
                    targets: vec!["settings/config".into()],
                },
            ],
            has_extends: false,
        }
    }

    #[test]
    fn alias_resolution_prefers_longest_prefix_and_exact_match() {
        let set = files(&["src/lib/a.ts", "src/special/b.ts", "src/settings/config.ts"]);
        let c = cfg();
        let r = ModuleResolver::new(&set, Some(&c));
        assert_eq!(r.resolve("x.ts", "@lib/a"), internal("src/lib/a.ts"));
        assert_eq!(
            r.resolve("x.ts", "@lib/special/b"),
            internal("src/special/b.ts")
        );
        assert_eq!(
            r.resolve("x.ts", "config"),
            internal("src/settings/config.ts")
        );
    }

    #[test]
    fn matched_alias_without_file_is_unresolved_not_external() {
        let set = files(&["src/lib/a.ts"]);
        let c = cfg();
        let r = ModuleResolver::new(&set, Some(&c));
        assert_eq!(
            r.resolve("x.ts", "@lib/missing"),
            Resolution::Unresolved(UnresolvedReason::NotFound)
        );
    }

    #[test]
    fn base_url_resolves_bare_specifiers_else_external() {
        let set = files(&["src/utils/x.ts"]);
        let c = cfg();
        let r = ModuleResolver::new(&set, Some(&c));
        assert_eq!(r.resolve("x.ts", "utils/x"), internal("src/utils/x.ts"));
        assert_eq!(
            r.resolve("x.ts", "lodash"),
            Resolution::External {
                package: "lodash".into()
            }
        );
    }
}
