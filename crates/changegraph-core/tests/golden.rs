//! Golden-file test: the committed sample report must equal what the analyzer really produces.
//!
//! `frontend/public/sample-report.json` is the report the viewer opens on start. This test builds
//! the scenario it describes (`fixtures/simple-ts-project` committed to a scratch Git repository,
//! then a comment appended to `src/auth/session.ts`), runs the real analyzer with timing disabled,
//! replaces the two machine-specific fields with placeholders, and compares the result with the
//! committed file. If they differ, either the analyzer's output changed on purpose or something broke.
//!
//! To accept an intended change (or to turn a hand-written sample into real output):
//!
//! ```text
//! UPDATE_GOLDEN=1 cargo test -p changegraph-core --test golden
//! ```
//!
//! Review the diff of the JSON file before committing it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use changegraph_core::{analyze, AnalysisConfig};
use serde_json::Value;

const PLACEHOLDER_ROOT: &str = "/example/simple-ts-project";
const PLACEHOLDER_HEAD: &str = "0000000000000000000000000000000000000000";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden_path() -> PathBuf {
    repo_root().join("frontend/public/sample-report.json")
}

fn copy_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &to);
        } else {
            fs::copy(entry.path(), &to).unwrap();
        }
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn actual_report() -> Value {
    let tmp = tempfile::TempDir::new().unwrap();
    let dir = tmp.path();
    copy_dir(&repo_root().join("fixtures/simple-ts-project"), dir);
    git(dir, &["init", "-q"]);
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(dir, &["config", "user.email", "golden@example.com"]);
    git(dir, &["config", "user.name", "Golden"]);
    git(dir, &["config", "commit.gpgsign", "false"]);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", "fixture"]);

    let session = dir.join("src/auth/session.ts");
    let mut text = fs::read_to_string(&session).unwrap();
    text.push_str("\n// sample change\n");
    fs::write(&session, text).unwrap();

    let mut cfg = AnalysisConfig::new(dir);
    cfg.record_timing = false;
    let report = analyze(&cfg).unwrap();

    let mut value = serde_json::to_value(&report).unwrap();
    value["repository"]["root"] = Value::String(PLACEHOLDER_ROOT.to_string());
    value["repository"]["head_commit"] = Value::String(PLACEHOLDER_HEAD.to_string());
    value
}

/// Path of the first difference between two JSON values, with both sides, or `None` if equal.
fn first_difference(actual: &Value, golden: &Value, path: &str) -> Option<String> {
    match (actual, golden) {
        (Value::Object(a), Value::Object(g)) => {
            for key in a.keys().chain(g.keys()) {
                match (a.get(key), g.get(key)) {
                    (Some(x), Some(y)) => {
                        if let Some(d) = first_difference(x, y, &format!("{path}.{key}")) {
                            return Some(d);
                        }
                    }
                    (Some(x), None) => return Some(format!("{path}.{key}: only in actual: {x}")),
                    (None, Some(y)) => return Some(format!("{path}.{key}: only in golden: {y}")),
                    (None, None) => {}
                }
            }
            None
        }
        (Value::Array(a), Value::Array(g)) => {
            if a.len() != g.len() {
                return Some(format!(
                    "{path}: actual has {} item(s), golden has {}",
                    a.len(),
                    g.len()
                ));
            }
            a.iter()
                .zip(g)
                .enumerate()
                .find_map(|(i, (x, y))| first_difference(x, y, &format!("{path}[{i}]")))
        }
        _ if actual == golden => None,
        _ => Some(format!(
            "{path}:\n    actual: {actual}\n    golden: {golden}"
        )),
    }
}

#[test]
fn sample_report_equals_real_analyzer_output() {
    let actual = actual_report();
    let path = golden_path();

    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&path, serde_json::to_string_pretty(&actual).unwrap() + "\n").unwrap();
        eprintln!("updated {}", path.display());
        return;
    }

    let golden: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    if let Some(diff) = first_difference(&actual, &golden, "$") {
        panic!(
            "frontend/public/sample-report.json no longer matches the analyzer's output.\nFirst difference at {diff}\n\n\
             If the new output is correct, regenerate with:\n  UPDATE_GOLDEN=1 cargo test -p changegraph-core --test golden\n\
             and review the diff."
        );
    }
}

#[test]
fn the_difference_finder_finds_differences() {
    use serde_json::json;
    let a = json!({"x": [1, 2, {"y": "a"}], "z": null});
    assert_eq!(first_difference(&a, &a.clone(), "$"), None);
    let b = json!({"x": [1, 2, {"y": "b"}], "z": null});
    assert!(first_difference(&a, &b, "$").unwrap().contains("$.x[2].y"));
    let c = json!({"x": [1, 2], "z": null});
    assert!(first_difference(&a, &c, "$").unwrap().contains("$.x"));
    let d = json!({"x": [1, 2, {"y": "a"}]});
    assert!(first_difference(&a, &d, "$")
        .unwrap()
        .contains("only in actual"));
}
