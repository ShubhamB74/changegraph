//! Tree-sitter based extraction of static imports and exports.
//!
//! Tree-sitter provides *syntax* only. This module extracts module-level facts from the
//! top-level statements of a file; it performs no type checking or semantic analysis.
//!
//! Recognised forms (top-level only):
//!
//! * `import ... from "x"`, `import "x"`, `import type ... from "x"`
//! * `export ... from "x"` (named) and `export * [as ns] from "x"` (re-exports)
//! * local exports: `export function/class/interface/type/enum/const/let/var`,
//!   `export { a, b as c }`, `export default ...`
//!
//! Not recognised (documented in `docs/limitations.md`): dynamic `import()`, `require()`,
//! `import x = require()`, `export =`, and imports nested inside `declare module` blocks.

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

/// Grammar selection by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    TypeScript,
    Tsx,
    JavaScript,
}

impl Language {
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        match ext.as_str() {
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            // The JavaScript grammar also parses JSX.
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            _ => None,
        }
    }

    /// The only place that touches grammar-crate constants. If a grammar crate changes its
    /// export names, this is the single function to adjust.
    fn grammar(self) -> tree_sitter::Language {
        match self {
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportKind {
    /// `import ... from "x"` / `import "x"`
    Import,
    /// `export { a } from "x"`
    ReExport,
    /// `export * from "x"` / `export * as ns from "x"`
    ReExportAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawImport {
    pub specifier: String,
    pub kind: ImportKind,
    /// Statement-level `import type` / `export type`. Per-specifier `{ type A }` is not
    /// detected, so such statements are treated as ordinary (runtime) imports.
    pub type_only: bool,
    /// 1-based line of the statement.
    pub line: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    /// No syntax errors.
    Ok,
    /// Syntax errors present; facts were extracted from the statements that parsed.
    Partial,
    /// The file could not be read or parsed; nothing was extracted.
    Failed,
}

#[derive(Debug, Clone)]
pub struct ParsedFile {
    pub imports: Vec<RawImport>,
    pub exports: Vec<String>,
    pub status: ParseStatus,
    /// 1-based line of the first syntax error, if any.
    pub first_error_line: Option<u32>,
}

impl ParsedFile {
    pub fn failed() -> Self {
        Self {
            imports: Vec::new(),
            exports: Vec::new(),
            status: ParseStatus::Failed,
            first_error_line: None,
        }
    }
}

/// Reusable parser; switches grammar only when the language changes.
pub struct SourceParser {
    parser: Parser,
    current: Option<Language>,
}

impl Default for SourceParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SourceParser {
    pub fn new() -> Self {
        Self {
            parser: Parser::new(),
            current: None,
        }
    }

    pub fn parse(&mut self, language: Language, source: &[u8]) -> ParsedFile {
        if self.current != Some(language) {
            if self.parser.set_language(&language.grammar()).is_err() {
                self.current = None;
                return ParsedFile::failed();
            }
            self.current = Some(language);
        }
        let Some(tree) = self.parser.parse(source, None) else {
            return ParsedFile::failed();
        };
        let root = tree.root_node();

        let mut file = ParsedFile {
            imports: Vec::new(),
            exports: Vec::new(),
            status: ParseStatus::Ok,
            first_error_line: None,
        };

        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            match child.kind() {
                "import_statement" => extract_import(child, source, &mut file),
                "export_statement" => extract_export(child, source, &mut file),
                _ => {}
            }
        }

        if root.has_error() {
            file.status = ParseStatus::Partial;
            file.first_error_line = first_error_line(root);
        }
        file
    }
}

fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

/// Unquote a string-literal node. Returns `None` for anything that is not a plain
/// single- or double-quoted literal.
fn string_value(node: Node, src: &[u8]) -> Option<String> {
    let text = node.utf8_text(src).ok()?.trim();
    let bytes = text.as_bytes();
    if text.len() >= 2
        && (bytes[0] == b'"' || bytes[0] == b'\'')
        && bytes[text.len() - 1] == bytes[0]
    {
        Some(text[1..text.len() - 1].to_string())
    } else {
        None
    }
}

/// Text of `node` that precedes `source_node` (e.g. `import type { A } from `).
fn head_text<'a>(node: Node, source_node: Node, src: &'a [u8]) -> &'a str {
    let (start, end) = (node.start_byte(), source_node.start_byte());
    if start > end || end > src.len() {
        return "";
    }
    std::str::from_utf8(&src[start..end]).unwrap_or("")
}

fn starts_with_word(s: &str, word: &str) -> bool {
    s.strip_prefix(word)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

/// If `rest` begins with a statement-level `type` modifier, return what follows it.
/// `import type from "x"` (a default import *named* `type`) is not a modifier.
fn strip_type_modifier(rest: &str) -> Option<&str> {
    let after = rest.strip_prefix("type")?;
    match after.chars().next() {
        Some('{') | Some('*') => Some(after),
        Some(c) if c.is_whitespace() => {
            let a = after.trim_start();
            if starts_with_word(a, "from") || a.starts_with(',') {
                None
            } else {
                Some(a)
            }
        }
        _ => None,
    }
}

fn extract_import(node: Node, src: &[u8], file: &mut ParsedFile) {
    let Some(source) = node.child_by_field_name("source") else {
        return;
    };
    let Some(specifier) = string_value(source, src) else {
        return;
    };
    let head = head_text(node, source, src).trim_start();
    let rest = head.strip_prefix("import").unwrap_or(head).trim_start();
    file.imports.push(RawImport {
        specifier,
        kind: ImportKind::Import,
        type_only: strip_type_modifier(rest).is_some(),
        line: line_of(node),
    });
}

fn extract_export(node: Node, src: &[u8], file: &mut ParsedFile) {
    if let Some(source) = node.child_by_field_name("source") {
        // Re-export: `export {..} from "x"` or `export * [as ns] from "x"`.
        let Some(specifier) = string_value(source, src) else {
            return;
        };
        let head = head_text(node, source, src).trim_start();
        let rest = head.strip_prefix("export").unwrap_or(head).trim_start();
        let (type_only, rest) = match strip_type_modifier(rest) {
            Some(after) => (true, after),
            None => (false, rest),
        };
        let kind = if rest.trim_start().starts_with('*') {
            ImportKind::ReExportAll
        } else {
            ImportKind::ReExport
        };
        file.imports.push(RawImport {
            specifier,
            kind,
            type_only,
            line: line_of(node),
        });
        return;
    }

    // Local exports.
    let mut cursor = node.walk();
    if node.children(&mut cursor).any(|c| c.kind() == "default") {
        file.exports.push("default".to_string());
        return;
    }

    if let Some(decl) = node.child_by_field_name("declaration") {
        match decl.kind() {
            "function_declaration"
            | "generator_function_declaration"
            | "class_declaration"
            | "abstract_class_declaration"
            | "interface_declaration"
            | "type_alias_declaration"
            | "enum_declaration" => {
                if let Some(name) = decl.child_by_field_name("name") {
                    push_text(name, src, &mut file.exports);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                let mut c = decl.walk();
                for d in decl.named_children(&mut c) {
                    if d.kind() == "variable_declarator" {
                        if let Some(name) = d.child_by_field_name("name") {
                            // Destructuring patterns are not tracked.
                            if name.kind() == "identifier" {
                                push_text(name, src, &mut file.exports);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        return;
    }

    let mut c = node.walk();
    for child in node.named_children(&mut c) {
        if child.kind() == "export_clause" {
            let mut cc = child.walk();
            for spec in child.named_children(&mut cc) {
                if spec.kind() == "export_specifier" {
                    let exported = spec
                        .child_by_field_name("alias")
                        .or_else(|| spec.child_by_field_name("name"));
                    if let Some(n) = exported {
                        push_text(n, src, &mut file.exports);
                    }
                }
            }
        }
    }
}

fn push_text(node: Node, src: &[u8], out: &mut Vec<String>) {
    if let Ok(text) = node.utf8_text(src) {
        out.push(text.to_string());
    }
}

/// Line of the first ERROR/MISSING node in source order.
fn first_error_line(root: Node) -> Option<u32> {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.is_error() || node.is_missing() {
            return Some(line_of(node));
        }
        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ts(src: &str) -> ParsedFile {
        SourceParser::new().parse(Language::TypeScript, src.as_bytes())
    }

    fn specs(f: &ParsedFile) -> Vec<(&str, ImportKind, bool)> {
        f.imports
            .iter()
            .map(|i| (i.specifier.as_str(), i.kind, i.type_only))
            .collect()
    }

    #[test]
    fn language_from_extension() {
        assert_eq!(Language::from_path("a.ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_path("a.mts"), Some(Language::TypeScript));
        assert_eq!(Language::from_path("a.tsx"), Some(Language::Tsx));
        assert_eq!(Language::from_path("a.jsx"), Some(Language::JavaScript));
        assert_eq!(Language::from_path("a.md"), None);
    }

    #[test]
    fn extracts_static_imports() {
        let f = parse_ts(
            "import a from './a';\nimport { b } from \"../b\";\nimport * as c from './c';\nimport './side';\n",
        );
        assert_eq!(f.status, ParseStatus::Ok);
        assert_eq!(
            specs(&f),
            vec![
                ("./a", ImportKind::Import, false),
                ("../b", ImportKind::Import, false),
                ("./c", ImportKind::Import, false),
                ("./side", ImportKind::Import, false),
            ]
        );
        assert_eq!(f.imports[1].line, 2);
    }

    #[test]
    fn detects_statement_level_type_only_imports() {
        let f = parse_ts(
            "import type { A } from './a';\nimport type B from './b';\nimport { type C } from './c';\nimport D from './d';\n",
        );
        assert_eq!(
            specs(&f),
            vec![
                ("./a", ImportKind::Import, true),
                ("./b", ImportKind::Import, true),
                ("./c", ImportKind::Import, false),
                ("./d", ImportKind::Import, false),
            ]
        );
    }

    #[test]
    fn type_modifier_helper_distinguishes_default_import_named_type() {
        assert!(strip_type_modifier("type { A } from").is_some());
        assert!(strip_type_modifier("type * as ns from").is_some());
        assert!(strip_type_modifier("type B from").is_some());
        assert!(strip_type_modifier("type fromX from").is_some());
        // `import type from "x"` / `import type, { a } from "x"`: `type` is the default binding.
        assert!(strip_type_modifier("type from").is_none());
        assert!(strip_type_modifier("type, { a } from").is_none());
        assert!(strip_type_modifier("typed from").is_none());
        assert!(strip_type_modifier("A from").is_none());
    }

    #[test]
    fn extracts_reexports() {
        let f = parse_ts(
            "export * from './all';\nexport * as ns from './ns';\nexport { a } from './named';\nexport type { T } from './types';\n",
        );
        assert_eq!(
            specs(&f),
            vec![
                ("./all", ImportKind::ReExportAll, false),
                ("./ns", ImportKind::ReExportAll, false),
                ("./named", ImportKind::ReExport, false),
                ("./types", ImportKind::ReExport, true),
            ]
        );
    }

    #[test]
    fn extracts_local_export_names() {
        let f = parse_ts(
            "export function f() {}\nexport class K {}\nexport interface I {}\nexport type T = number;\nexport enum E { A }\nexport const x = 1, y = 2;\nconst z = 3;\nexport { z, z as zz };\nexport default function () {}\n",
        );
        assert_eq!(
            f.exports,
            vec!["f", "K", "I", "T", "E", "x", "y", "z", "zz", "default"]
        );
        assert!(f.imports.is_empty());
    }

    #[test]
    fn plain_declarations_are_not_exports() {
        let f = parse_ts("function hidden() {}\nconst also = 1;\n");
        assert!(f.exports.is_empty());
    }

    #[test]
    fn syntax_errors_yield_partial_status_with_line() {
        let f = parse_ts("import { a } from './a';\nexport const x = ;\n");
        assert_eq!(f.status, ParseStatus::Partial);
        assert!(f.first_error_line.is_some());
    }

    #[test]
    fn javascript_and_tsx_grammars_work() {
        let mut p = SourceParser::new();
        let js = p.parse(
            Language::JavaScript,
            b"import a from './a.js';\nconst el = <div/>;\n",
        );
        assert_eq!(js.status, ParseStatus::Ok);
        assert_eq!(js.imports.len(), 1);
        let tsx = p.parse(
            Language::Tsx,
            b"import { C } from './c';\nexport const v = <C />;\n",
        );
        assert_eq!(tsx.status, ParseStatus::Ok);
        assert_eq!(tsx.exports, vec!["v"]);
    }
}
