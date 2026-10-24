//! End-to-end tests that run the compiled `changegraph` binary.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

fn init_repo() -> TempDir {
    let dir = TempDir::new().unwrap();
    git(dir.path(), &["init", "-q"]);
    git(dir.path(), &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    git(dir.path(), &["config", "user.name", "Test"]);
    git(dir.path(), &["config", "commit.gpgsign", "false"]);
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.ts"), "export const a = 1;\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-q", "-m", "init"]);
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_changegraph"))
        .args(args)
        .output()
        .unwrap()
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap()
}

#[test]
fn json_output_has_schema_version_and_changed_files() {
    let repo = init_repo();
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();

    let out = run(&["analyze", repo.path().to_str().unwrap(), "--format", "json"]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is valid JSON");
    assert_eq!(v["schema_version"], "0.4.0");
    assert_eq!(v["comparison"]["mode"], "working_tree");
    assert_eq!(v["changed_files"][0]["path"], "src/a.ts");
    assert_eq!(v["changed_files"][0]["status"], "modified");
}

#[test]
fn json_output_includes_dependency_graph() {
    let repo = init_repo();
    fs::write(
        repo.path().join("src/b.ts"),
        "import { a } from './a';\nimport x from 'left-pad';\nimport y from './nope';\nexport const b = a;\n",
    )
    .unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "add b"]);

    let out = run(&["analyze", repo.path().to_str().unwrap(), "--format", "json"]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let g = &v["graph"];
    assert_eq!(g["stats"]["files"], 2);
    assert_eq!(g["edges"][0]["from"], "src/b.ts");
    assert_eq!(g["edges"][0]["to"], "src/a.ts");
    assert_eq!(g["external_dependencies"][0]["package"], "left-pad");
    assert_eq!(g["unresolved_imports"][0]["specifier"], "./nope");
    assert_eq!(g["unresolved_imports"][0]["reason"], "not_found");
}

#[test]
fn json_output_includes_impact_with_explanations() {
    let repo = init_repo(); // src/a.ts
    fs::write(repo.path().join("src/b.ts"), "import { a } from './a';\nexport const b = a;\n").unwrap();
    fs::write(repo.path().join("src/c.ts"), "import { b } from './b';\nexport const c = b;\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "add b, c"]);
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();

    let out = run(&["analyze", repo.path().to_str().unwrap(), "--format", "json"]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let im = &v["impact"];
    assert_eq!(im["summary"]["changed_files"], 1);
    assert_eq!(im["summary"]["affected"], 2);
    assert_eq!(im["affected"][0]["path"], "src/b.ts");
    assert_eq!(im["affected"][0]["classification"], "direct");
    assert_eq!(im["affected"][0]["rank"], 1);
    assert_eq!(im["affected"][1]["path"], "src/c.ts");
    assert_eq!(im["affected"][1]["classification"], "transitive");
    assert_eq!(
        im["affected"][1]["path_from_changed"],
        serde_json::json!(["src/a.ts", "src/b.ts", "src/c.ts"])
    );
    assert!(im["affected"][1]["explanation"].as_str().unwrap().contains("distance 2"));
    assert_eq!(im["depth_limited"], false);
}

#[test]
fn max_depth_limits_impact_and_is_reported() {
    let repo = init_repo();
    fs::write(repo.path().join("src/b.ts"), "import { a } from './a';\nexport const b = a;\n").unwrap();
    fs::write(repo.path().join("src/c.ts"), "import { b } from './b';\nexport const c = b;\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "add b, c"]);
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();

    let out = run(&[
        "analyze",
        repo.path().to_str().unwrap(),
        "--max-depth",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(code(&out), 0);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["impact"]["summary"]["affected"], 1);
    assert_eq!(v["impact"]["depth_limited"], true);
    let codes: Vec<_> = v["warnings"].as_array().unwrap().iter().map(|w| w["code"].clone()).collect();
    assert!(codes.contains(&serde_json::json!("impact_depth_limited")));

    // Not a number -> usage error from clap (exit code 2).
    let out = run(&["analyze", repo.path().to_str().unwrap(), "--max-depth", "abc"]);
    assert_eq!(code(&out), 2);
}

#[test]
fn text_output_shows_impact_section() {
    let repo = init_repo();
    fs::write(repo.path().join("src/b.ts"), "import { a } from './a';\nexport const b = a;\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "add b"]);
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();
    let out = run(&["analyze", repo.path().to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Impact"), "{text}");
    assert!(text.contains("Direct dependent of changed file src/a.ts"), "{text}");
}

#[test]
fn text_output_mentions_changed_file() {
    let repo = init_repo();
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();
    let out = run(&["analyze", repo.path().to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("src/a.ts"), "{text}");
}

#[test]
fn base_flag_uses_merge_base_mode() {
    let repo = init_repo();
    fs::write(repo.path().join("src/b.ts"), "export const b = 1;\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "add b"]);

    let out = run(&[
        "analyze",
        repo.path().to_str().unwrap(),
        "--base",
        "HEAD~1",
        "--format",
        "json",
    ]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["comparison"]["mode"], "merge_base");
    assert_eq!(v["changed_files"][0]["path"], "src/b.ts");
    assert_eq!(v["changed_files"][0]["status"], "added");
}

#[test]
fn output_flag_writes_report_to_file() {
    let repo = init_repo();
    let out_dir = TempDir::new().unwrap();
    let file = out_dir.path().join("report.json");
    let out = run(&[
        "analyze",
        repo.path().to_str().unwrap(),
        "--format",
        "json",
        "--output",
        file.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    assert!(out.stdout.is_empty());
    let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(file).unwrap()).unwrap();
    assert_eq!(v["schema_version"], "0.4.0");
}

#[test]
fn exit_code_3_for_missing_path_and_non_git_directory() {
    let out = run(&["analyze", "/definitely/not/a/real/path"]);
    assert_eq!(code(&out), 3);
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not exist"));

    let plain = TempDir::new().unwrap();
    let out = run(&["analyze", plain.path().to_str().unwrap()]);
    assert_eq!(code(&out), 3);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not inside a Git repository"), "{stderr}");
    assert!(stderr.contains("hint:"), "{stderr}");
}

#[test]
fn exit_code_4_for_unknown_base() {
    let repo = init_repo();
    let out = run(&["analyze", repo.path().to_str().unwrap(), "--base", "nope"]);
    assert_eq!(code(&out), 4);
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope"));
}

#[test]
fn exit_code_2_for_target_without_base() {
    let repo = init_repo();
    let out = run(&["analyze", repo.path().to_str().unwrap(), "--target", "HEAD"]);
    assert_eq!(code(&out), 2);
}

#[test]
fn help_lists_main_options() {
    let out = run(&["analyze", "--help"]);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in ["--base", "--target", "--format", "--exclude", "--output"] {
        assert!(text.contains(flag), "missing {flag} in help");
    }
}

// ---------------------------------------------------------------------------------------
// Phase 4: test recommendations
// ---------------------------------------------------------------------------------------

fn commit_all(dir: &Path, msg: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", msg]);
}

/// src/a.ts plus tests/a.test.ts importing it; then modify src/a.ts (uncommitted).
fn repo_with_test() -> TempDir {
    let repo = init_repo();
    fs::create_dir_all(repo.path().join("tests")).unwrap();
    fs::write(
        repo.path().join("tests/a.test.ts"),
        "import { a } from '../src/a';\ntest('a', () => expect(a).toBe(1));\n",
    )
    .unwrap();
    commit_all(repo.path(), "add test");
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();
    repo
}

fn json_of(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).expect("stdout is valid JSON")
}

#[test]
fn json_includes_explained_test_recommendations() {
    let repo = repo_with_test();
    let out = run(&["analyze", repo.path().to_str().unwrap(), "--format", "json"]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let t = &json_of(&out)["test_recommendations"];
    assert_eq!(t["recommendations"][0]["path"], "tests/a.test.ts");
    assert_eq!(t["recommendations"][0]["basis"], "known");
    assert_eq!(t["recommendations"][0]["methods"][0], "direct_import");
    assert!(t["recommendations"][0]["explanation"].as_str().unwrap().starts_with("Known relationship"));
    assert_eq!(t["completeness"]["complete"], true);
    assert_eq!(t["summary"]["recommended"], 1);
}

#[test]
fn text_output_shows_tests_and_incompleteness() {
    let repo = init_repo(); // no tests at all
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();
    let out = run(&["analyze", repo.path().to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("Recommended tests"), "{text}");
    assert!(text.contains("TEST SELECTION IS INCOMPLETE"), "{text}");
    assert!(text.contains("NOT evidence that no tests are needed"), "{text}");
}

#[test]
fn coverage_map_is_used_and_validated() {
    let repo = init_repo(); // src/a.ts only
    fs::create_dir_all(repo.path().join("tests")).unwrap();
    fs::write(repo.path().join("tests/slow.test.ts"), "test('x', () => {});\n").unwrap();
    commit_all(repo.path(), "add test with no import");
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();

    let dir = TempDir::new().unwrap();
    let map = dir.path().join("map.json");
    fs::write(&map, r#"{"version":1,"tests":{"tests/slow.test.ts":["src/a.ts"]}}"#).unwrap();
    let out = run(&[
        "analyze", repo.path().to_str().unwrap(), "--format", "json",
        "--coverage-map", map.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
    let t = &json_of(&out)["test_recommendations"];
    assert_eq!(t["recommendations"][0]["path"], "tests/slow.test.ts");
    assert_eq!(t["recommendations"][0]["methods"][0], "coverage_map");
    assert_eq!(t["coverage_map"]["tests"], 1);

    // Malformed map: usage/config error (2). Missing map: path error (3).
    fs::write(&map, r#"{"version":9}"#).unwrap();
    let out = run(&["analyze", repo.path().to_str().unwrap(), "--coverage-map", map.to_str().unwrap()]);
    assert_eq!(code(&out), 2);
    assert!(String::from_utf8_lossy(&out.stderr).contains("hint:"));
    let out = run(&["analyze", repo.path().to_str().unwrap(), "--coverage-map", "/no/such/map.json"]);
    assert_eq!(code(&out), 3);
}

// ---------------------------------------------------------------------------------------
// Phase 5: output options and exit codes
// ---------------------------------------------------------------------------------------

#[test]
fn fail_on_incomplete_tests_exits_6_but_still_writes_the_report() {
    let repo = init_repo(); // changing src/a.ts with no tests => incomplete
    fs::write(repo.path().join("src/a.ts"), "export const a = 2;\n").unwrap();
    let out = run(&[
        "analyze", repo.path().to_str().unwrap(), "--format", "json", "--fail-on", "incomplete-tests",
    ]);
    assert_eq!(code(&out), 6);
    assert_eq!(json_of(&out)["test_recommendations"]["completeness"]["complete"], false);
    assert!(String::from_utf8_lossy(&out.stderr).contains("fail-on condition met"));
}

#[test]
fn fail_on_incomplete_tests_exits_0_when_selection_is_complete() {
    let repo = repo_with_test();
    let out = run(&[
        "analyze", repo.path().to_str().unwrap(), "--format", "json", "--fail-on", "incomplete-tests",
    ]);
    assert_eq!(code(&out), 0, "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn fail_on_warnings_exits_6_when_there_are_warnings() {
    let repo = init_repo();
    fs::write(repo.path().join("src/b.ts"), "export const b = 1;\n").unwrap();
    commit_all(repo.path(), "b");
    fs::write(repo.path().join("dirty.ts"), "x\n").unwrap(); // dirty tree + --base => warning
    let args = ["analyze", repo.path().to_str().unwrap(), "--base", "HEAD~1", "--format", "json"];
    assert_eq!(code(&run(&args)), 0);
    let mut strict = args.to_vec();
    strict.extend(["--fail-on", "warnings"]);
    assert_eq!(code(&run(&strict)), 6);
}

#[test]
fn compact_and_no_timing_give_small_byte_identical_json() {
    let repo = repo_with_test();
    let args = [
        "analyze", repo.path().to_str().unwrap(), "--format", "json", "--compact", "--no-timing",
    ];
    let a = run(&args);
    let b = run(&args);
    assert_eq!(code(&a), 0);
    assert_eq!(a.stdout, b.stdout, "identical state + options must give identical bytes");
    let text = String::from_utf8_lossy(&a.stdout);
    assert_eq!(text.trim_end().lines().count(), 1, "compact JSON is a single line");
    assert!(json_of(&a)["execution"]["duration_ms"].is_null());

    let pretty = run(&["analyze", repo.path().to_str().unwrap(), "--format", "json", "--no-timing"]);
    assert!(pretty.stdout.len() > a.stdout.len());

    // --compact without --format json is a configuration error.
    let out = run(&["analyze", repo.path().to_str().unwrap(), "--compact"]);
    assert_eq!(code(&out), 2);
}

#[test]
fn output_file_is_written_atomically_and_only_on_success() {
    let repo = repo_with_test();
    let out_dir = TempDir::new().unwrap();
    let file = out_dir.path().join("report.json");
    let out = run(&[
        "analyze", repo.path().to_str().unwrap(), "--format", "json", "--output", file.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    assert!(file.exists());
    let leftovers: Vec<_> = fs::read_dir(out_dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, vec!["report.json"], "no temporary files left behind");

    // A failing analysis must not create or clobber the output file.
    let failing = out_dir.path().join("never.json");
    let out = run(&[
        "analyze", repo.path().to_str().unwrap(), "--base", "nope", "--output", failing.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 4);
    assert!(!failing.exists());
}

#[test]
fn help_documents_new_options_and_exit_codes() {
    let out = run(&["--help"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("EXIT CODES"), "{text}");
    assert!(text.contains("6  analysis succeeded"), "{text}");
    let out = run(&["analyze", "--help"]);
    let text = String::from_utf8_lossy(&out.stdout);
    for flag in ["--coverage-map", "--compact", "--no-timing", "--fail-on", "--max-depth"] {
        assert!(text.contains(flag), "missing {flag}");
    }
}
