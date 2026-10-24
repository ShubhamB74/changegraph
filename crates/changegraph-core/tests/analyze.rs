mod common;

use changegraph_core::git_diff::ChangeStatus;
use changegraph_core::report::WarningCode;
use changegraph_core::{analyze, AnalysisConfig, ChangeGraphError, ComparisonMode, Report};
use common::TestRepo;

fn changes(report: &Report) -> Vec<(&str, ChangeStatus)> {
    report
        .changed_files
        .iter()
        .map(|f| (f.path.as_str(), f.status))
        .collect()
}

#[test]
fn working_tree_mode_reports_modified_deleted_and_untracked() {
    let repo = TestRepo::new();
    repo.write("a.ts", "export const a = 1;\n");
    repo.write("b.ts", "export const b = 1;\n");
    repo.commit_all("init");

    repo.write("a.ts", "export const a = 2;\n");
    repo.remove("b.ts");
    repo.write("c.ts", "export const c = 1;\n");

    let report = analyze(&AnalysisConfig::new(repo.path())).unwrap();
    assert_eq!(report.comparison.mode, ComparisonMode::WorkingTree);
    assert_eq!(
        changes(&report),
        vec![
            ("a.ts", ChangeStatus::Modified),
            ("b.ts", ChangeStatus::Deleted),
            ("c.ts", ChangeStatus::Untracked),
        ]
    );
}

#[test]
fn clean_working_tree_has_no_changes() {
    let repo = TestRepo::new();
    repo.write("a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    let report = analyze(&AnalysisConfig::new(repo.path())).unwrap();
    assert!(report.changed_files.is_empty());
}

#[test]
fn merge_base_mode_excludes_changes_made_only_on_base() {
    let repo = TestRepo::new();
    repo.write("a.ts", "export const a = 1;\n");
    repo.write("b.ts", "export const b = 1;\n");
    repo.commit_all("init");

    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("a.ts", "export const a = 2;\n");
    repo.commit_all("feature change");

    repo.git(&["checkout", "-q", "main"]);
    repo.write("b.ts", "export const b = 2;\n");
    repo.commit_all("main advanced");

    repo.git(&["checkout", "-q", "feature"]);

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.base = Some("main".to_string());
    let report = analyze(&cfg).unwrap();

    assert_eq!(report.comparison.mode, ComparisonMode::MergeBase);
    assert!(report.comparison.merge_base_commit.is_some());
    // Only the feature branch's own change; `b.ts` changed on main after the fork.
    assert_eq!(changes(&report), vec![("a.ts", ChangeStatus::Modified)]);
    // No dirty-tree or graph/target warnings. (The repo has no tests, so the only expected
    // warning is the honest "test selection is incomplete".)
    assert!(report
        .warnings
        .iter()
        .all(|w| w.code == WarningCode::TestSelectionIncomplete));
}

#[test]
fn merge_base_mode_warns_about_uncommitted_changes_and_does_not_include_them() {
    let repo = TestRepo::new();
    repo.write("a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    repo.write("a.ts", "export const a = 2;\n");
    repo.commit_all("second");
    repo.write("dirty.ts", "export const d = 1;\n");

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.base = Some("HEAD~1".to_string());
    let report = analyze(&cfg).unwrap();

    assert_eq!(changes(&report), vec![("a.ts", ChangeStatus::Modified)]);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::DirtyWorkingTree));
}

#[test]
fn detects_renames_with_old_path() {
    let repo = TestRepo::new();
    repo.write("old.ts", "export const value = 'unchanged content';\n");
    repo.commit_all("init");
    repo.git(&["mv", "old.ts", "new.ts"]);
    repo.commit_all("rename");

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.base = Some("HEAD~1".to_string());
    let report = analyze(&cfg).unwrap();

    assert_eq!(report.changed_files.len(), 1);
    let f = &report.changed_files[0];
    assert_eq!(f.status, ChangeStatus::Renamed);
    assert_eq!(f.path, "new.ts");
    assert_eq!(f.old_path.as_deref(), Some("old.ts"));
}

#[test]
fn excluded_changed_files_are_listed_but_flagged() {
    let repo = TestRepo::new();
    repo.write("a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    repo.write("dist/out.js", "module.exports = {};\n");

    let report = analyze(&AnalysisConfig::new(repo.path())).unwrap();
    let f = report
        .changed_files
        .iter()
        .find(|f| f.path == "dist/out.js")
        .expect("excluded file must still be listed");
    assert!(f.excluded);
    // ...and it is not part of discovery.
    assert!(report.graph.nodes.iter().all(|n| n.path != "dist/out.js"));
}

#[test]
fn unknown_base_is_a_revision_error() {
    let repo = TestRepo::new();
    repo.write("a.ts", "x\n");
    repo.commit_all("init");

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.base = Some("does-not-exist".to_string());
    let err = analyze(&cfg).unwrap_err();
    assert!(matches!(err, ChangeGraphError::RevisionNotFound { .. }));
    assert_eq!(err.exit_code(), 4);
    assert!(err.hint().is_some());
}

#[test]
fn option_like_revisions_are_rejected() {
    let repo = TestRepo::new();
    repo.write("a.ts", "x\n");
    repo.commit_all("init");

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.base = Some("--output=/tmp/pwned".to_string());
    let err = analyze(&cfg).unwrap_err();
    assert!(matches!(err, ChangeGraphError::InvalidRevision { .. }));
}

#[test]
fn repository_without_commits_is_a_revision_error() {
    let repo = TestRepo::new();
    let err = analyze(&AnalysisConfig::new(repo.path())).unwrap_err();
    assert!(matches!(err, ChangeGraphError::RevisionNotFound { .. }));
}

#[test]
fn target_without_base_is_invalid_config() {
    let repo = TestRepo::new();
    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.target = Some("HEAD".to_string());
    let err = analyze(&cfg).unwrap_err();
    assert!(matches!(err, ChangeGraphError::InvalidConfig(_)));
    assert_eq!(err.exit_code(), 2);
}

#[test]
fn non_git_directory_is_rejected() {
    let dir = tempfile::TempDir::new().unwrap();
    let err = analyze(&AnalysisConfig::new(dir.path())).unwrap_err();
    assert!(matches!(err, ChangeGraphError::NotAGitRepository(_)));
    assert_eq!(err.exit_code(), 3);
}

#[test]
fn missing_path_and_file_path_are_rejected() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("nope");
    let err = analyze(&AnalysisConfig::new(&missing)).unwrap_err();
    assert!(matches!(err, ChangeGraphError::PathNotFound(_)));

    let file = dir.path().join("f.txt");
    std::fs::write(&file, "x").unwrap();
    let err = analyze(&AnalysisConfig::new(&file)).unwrap_err();
    assert!(matches!(err, ChangeGraphError::NotADirectory(_)));
}

#[test]
fn reports_are_deterministic_except_for_duration() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.write("src/a.test.ts", "import './a';\n");
    repo.commit_all("init");
    repo.write("src/a.ts", "export const a = 2;\n");
    repo.write("src/z.ts", "export const z = 1;\n");

    let cfg = AnalysisConfig::new(repo.path());
    let mut first = analyze(&cfg).unwrap();
    let mut second = analyze(&cfg).unwrap();
    first.execution.duration_ms = None;
    second.execution.duration_ms = None;
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
}

#[test]
fn warns_when_graph_is_not_built_from_the_target_revision() {
    let repo = TestRepo::new();
    repo.write("a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    repo.git(&["checkout", "-q", "-b", "feature"]);
    repo.write("a.ts", "export const a = 2;\n");
    repo.commit_all("feature change");
    repo.git(&["checkout", "-q", "main"]); // working tree is NOT the target

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.base = Some("main".to_string());
    cfg.target = Some("feature".to_string());
    let report = analyze(&cfg).unwrap();

    assert_eq!(changes(&report), vec![("a.ts", ChangeStatus::Modified)]);
    assert!(report
        .warnings
        .iter()
        .any(|w| w.code == WarningCode::GraphTargetMismatch));
}

#[test]
fn impact_section_is_populated_from_real_git_changes() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.write(
        "src/b.ts",
        "import { a } from './a';\nexport const b = a;\n",
    );
    repo.write("src/b.test.ts", "import { b } from './b';\n");
    repo.commit_all("init");
    repo.write("src/a.ts", "export const a = 2;\n");

    let report = analyze(&AnalysisConfig::new(repo.path())).unwrap();
    let affected: Vec<_> = report
        .impact
        .affected
        .iter()
        .map(|a| (a.path.as_str(), a.distance))
        .collect();
    assert_eq!(affected, vec![("src/b.ts", 1), ("src/b.test.ts", 2)]);
    assert_eq!(report.impact.summary.affected_tests, 1);
}

#[test]
fn deleting_an_imported_file_flags_its_importers() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.write(
        "src/b.ts",
        "import { a } from './a';\nexport const b = a;\n",
    );
    repo.commit_all("init");
    repo.remove("src/a.ts");

    let report = analyze(&AnalysisConfig::new(repo.path())).unwrap();
    assert_eq!(changes(&report), vec![("src/a.ts", ChangeStatus::Deleted)]);
    assert_eq!(report.impact.affected.len(), 1);
    assert_eq!(report.impact.affected[0].path, "src/b.ts");
    assert!(report.impact.affected[0]
        .explanation
        .contains("deleted file src/a.ts"));
}

#[test]
fn test_recommendations_and_coverage_map_flow_through_analyze() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.write("src/a.test.ts", "import { a } from './a';\n");
    repo.write("src/slow.test.ts", "test('slow', () => {});\n");
    repo.commit_all("init");
    repo.write("src/a.ts", "export const a = 2;\n");

    let dir = tempfile::TempDir::new().unwrap();
    let map = dir.path().join("map.json");
    std::fs::write(
        &map,
        r#"{"version":1,"tests":{"src/slow.test.ts":["src/a.ts"]}}"#,
    )
    .unwrap();

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.coverage_map = Some(map);
    let report = analyze(&cfg).unwrap();
    let recs: Vec<_> = report
        .test_recommendations
        .recommendations
        .iter()
        .map(|r| r.path.as_str())
        .collect();
    assert_eq!(recs, vec!["src/a.test.ts", "src/slow.test.ts"]);
    assert!(report.test_recommendations.completeness.complete);
}

#[test]
fn bad_coverage_map_fails_fast_with_actionable_errors() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    let dir = tempfile::TempDir::new().unwrap();

    let mut cfg = AnalysisConfig::new(repo.path());
    cfg.coverage_map = Some(dir.path().join("missing.json"));
    let err = analyze(&cfg).unwrap_err();
    assert!(matches!(err, ChangeGraphError::PathNotFound(_)));

    let bad = dir.path().join("bad.json");
    std::fs::write(&bad, "{\"version\": 3}").unwrap();
    cfg.coverage_map = Some(bad);
    let err = analyze(&cfg).unwrap_err();
    assert!(matches!(err, ChangeGraphError::InvalidCoverageMap { .. }));
    assert_eq!(err.exit_code(), 2);
    assert!(err.hint().is_some());
}

#[test]
fn timing_can_be_disabled() {
    let repo = TestRepo::new();
    repo.write("src/a.ts", "export const a = 1;\n");
    repo.commit_all("init");
    let mut cfg = AnalysisConfig::new(repo.path());
    assert!(analyze(&cfg).unwrap().execution.duration_ms.is_some());
    cfg.record_timing = false;
    assert!(analyze(&cfg).unwrap().execution.duration_ms.is_none());
}
