//! Minimal `tsconfig.json` reader for path-alias resolution.
//!
//! Supported: `compilerOptions.baseUrl` and `compilerOptions.paths` (single `*` wildcard),
//! in the repository-root `tsconfig.json`, with JSONC comments and trailing commas.
//! Not supported: `extends`, project references, multiple tsconfig files, `rootDirs`.

use std::path::Path;

use serde_json::Value;

use crate::report::{Warning, WarningCode};
use crate::resolver::normalize_join;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathAlias {
    /// Pattern such as `@lib/*` or `config`.
    pub pattern: String,
    /// Target templates relative to [`TsConfig::base_dir`].
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TsConfig {
    /// Repository-relative directory that `paths` targets are resolved against
    /// (`""` is the repository root).
    pub base_dir: String,
    /// True if `baseUrl` was set: non-relative specifiers are also tried against `base_dir`.
    pub has_base_url: bool,
    pub aliases: Vec<PathAlias>,
    /// True if the file has an `extends` key that we do not follow.
    pub has_extends: bool,
}

/// Load `<root>/tsconfig.json`. A missing file is not an issue; a malformed one yields a
/// warning and disables alias resolution.
pub fn load(root: &Path) -> (Option<TsConfig>, Vec<Warning>) {
    let path = root.join("tsconfig.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (None, Vec::new()),
        Err(e) => {
            return (
                None,
                vec![issue(format!(
                    "could not read tsconfig.json: {e}; path aliases are disabled"
                ))],
            )
        }
    };
    match parse(&text) {
        Ok(cfg) => {
            let mut warnings = Vec::new();
            if cfg.has_extends {
                warnings.push(issue(
                    "tsconfig.json uses `extends`, which is not followed; aliases defined in \
                     parent configs are not resolved"
                        .to_string(),
                ));
            }
            (Some(cfg), warnings)
        }
        Err(msg) => (
            None,
            vec![issue(format!(
                "could not parse tsconfig.json: {msg}; path aliases are disabled"
            ))],
        ),
    }
}

fn issue(message: String) -> Warning {
    Warning {
        code: WarningCode::TsconfigIssue,
        message,
    }
}

pub fn parse(text: &str) -> Result<TsConfig, String> {
    let value: Value = serde_json::from_str(&strip_jsonc(text)).map_err(|e| e.to_string())?;
    let mut cfg = TsConfig {
        has_extends: value.get("extends").is_some(),
        ..TsConfig::default()
    };
    let Some(opts) = value.get("compilerOptions") else {
        return Ok(cfg);
    };

    if let Some(base_url) = opts.get("baseUrl").and_then(Value::as_str) {
        cfg.base_dir = normalize_join("", base_url)
            .ok_or_else(|| "baseUrl points outside the repository".to_string())?;
        cfg.has_base_url = true;
    }

    if let Some(paths) = opts.get("paths").and_then(Value::as_object) {
        for (pattern, targets) in paths {
            let targets: Vec<String> = targets
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            cfg.aliases.push(PathAlias {
                pattern: pattern.clone(),
                targets,
            });
        }
    }
    Ok(cfg)
}

/// Convert JSONC (comments + trailing commas) to plain JSON.
pub fn strip_jsonc(input: &str) -> String {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_str = false;

    while let Some(c) = chars.next() {
        if in_str {
            out.push(c);
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_str = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                while let Some(&n) = chars.peek() {
                    if n == '\n' {
                        break;
                    }
                    chars.next();
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    remove_trailing_commas(&out)
}

fn remove_trailing_commas(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut in_str = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                i += 1;
                out.push(chars[i]);
            } else if c == '"' {
                in_str = false;
            }
        } else if c == '"' {
            in_str = true;
            out.push(c);
        } else if c == ',' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let trailing = j < chars.len() && (chars[j] == '}' || chars[j] == ']');
            if !trailing {
                out.push(c);
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_comments_and_trailing_commas_but_not_inside_strings() {
        let src = r#"{
          // line comment
          "a": "http://example.com/*not a comment*/", /* block */
          "b": [1, 2,],
        }"#;
        let v: Value = serde_json::from_str(&strip_jsonc(src)).unwrap();
        assert_eq!(v["a"], "http://example.com/*not a comment*/");
        assert_eq!(v["b"], serde_json::json!([1, 2]));
    }

    #[test]
    fn parses_base_url_and_paths() {
        let cfg = parse(
            r#"{ "compilerOptions": { "baseUrl": "./src", "paths": { "@lib/*": ["lib/*"] } } }"#,
        )
        .unwrap();
        assert_eq!(cfg.base_dir, "src");
        assert!(cfg.has_base_url);
        assert_eq!(cfg.aliases.len(), 1);
        assert_eq!(cfg.aliases[0].pattern, "@lib/*");
        assert_eq!(cfg.aliases[0].targets, vec!["lib/*"]);
    }

    #[test]
    fn extends_is_flagged_and_missing_options_are_fine() {
        let cfg = parse(r#"{ "extends": "./base.json" }"#).unwrap();
        assert!(cfg.has_extends);
        assert!(cfg.aliases.is_empty());
    }

    #[test]
    fn malformed_json_is_an_error() {
        assert!(parse("{ not json").is_err());
    }
}
