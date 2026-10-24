//! End-to-end tests that run the compiled `changegraph` binary.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
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
    assert_eq!(v["schema_version"], "0.1.0");
    assert_eq!(v["comparison"]["mode"], "working_tree");
    assert_eq!(v["changed_files"][0]["path"], "src/a.ts");
    assert_eq!(v["changed_files"][0]["status"], "modified");
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
    assert_eq!(v["schema_version"], "0.1.0");
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
