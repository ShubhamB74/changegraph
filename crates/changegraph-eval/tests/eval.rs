//! The harness's own regression tests. They also act as the project's accuracy regression
//! suite: any change that makes extraction or impact analysis disagree with the labels fails.

use std::fs;
use std::path::PathBuf;

use changegraph_eval::gitutil::{commit_all, git, init_repo, write_file};
use changegraph_eval::historical;
use changegraph_eval::scenario::{aggregate, load_scenarios, run_scenario};
use changegraph_eval::synthetic::SyntheticSpec;
use changegraph_eval::synthetic_eval::evaluate;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn labeled_scenarios_agree_with_the_tool() {
    let scenarios = load_scenarios(&repo_root().join("eval/scenarios")).unwrap();
    assert!(
        scenarios.len() >= 10,
        "expected the committed scenario set, found {}",
        scenarios.len()
    );

    let mut results = Vec::new();
    let mut failures = Vec::new();
    for s in &scenarios {
        let r = run_scenario(s, &repo_root()).unwrap();
        if !r.affected.counts().is_exact() {
            failures.push(format!(
                "{}: affected missing {:?}, unexpected {:?}",
                r.name, r.missing_affected, r.unexpected_affected
            ));
        }
        if !r.tests_known.counts().is_exact() {
            failures.push(format!(
                "{}: known-basis tests differ from labels: {:?}",
                r.name, r.tests_known
            ));
        }
        if let Some(u) = &r.uncertain {
            if !u.counts().is_exact() {
                failures.push(format!(
                    "{}: uncertain set differs from labels: {:?}",
                    r.name, u
                ));
            }
        }
        results.push(r);
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));

    let agg = aggregate(&results);
    assert_eq!(agg.exact_affected, agg.scenarios);
    assert!(
        agg.affected.tp > 10,
        "the scenario set should exercise real dependents"
    );
}

#[test]
fn synthetic_ground_truth_is_matched_exactly() {
    for (files, seed) in [(150, 1), (400, 7)] {
        let r = evaluate(&SyntheticSpec::new(files, seed)).unwrap();
        // Guard against a vacuous pass.
        assert!(
            r.true_edges > files / 2,
            "{} edges for {} files",
            r.true_edges,
            files
        );
        assert!(r.targets[0].truth_affected > 0);

        assert!(
            r.extraction_edges.counts().is_exact(),
            "{files} files: edges {:?}",
            r.extraction_edges
        );
        assert!(
            r.extraction_external.counts().is_exact(),
            "{files} files: external {:?}",
            r.extraction_external
        );
        assert!(
            r.extraction_unresolved.counts().is_exact(),
            "{files} files: unresolved {:?}",
            r.extraction_unresolved
        );
        assert!(
            r.extraction_unresolved.tp > 0 || files < 100,
            "unresolved imports should be exercised"
        );
        for t in &r.targets {
            assert!(
                t.impact.counts().is_exact(),
                "{files} files, change {}: {:?}",
                t.target,
                t.impact
            );
            assert_eq!(
                t.distance_exact, t.truth_affected,
                "{}: shortest distances differ",
                t.target
            );
            assert!(
                t.tests_known.counts().is_exact(),
                "{}: {:?}",
                t.target,
                t.tests_known
            );
        }
    }
}

#[test]
fn historical_evaluation_counts_found_and_missed_tests() {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_repo(&repo).unwrap();
    write_file(&repo, "src/a.ts", "export const a = 1;\n").unwrap();
    write_file(
        &repo,
        "src/b.ts",
        "import { a } from './a';\nexport const b = a;\n",
    )
    .unwrap();
    write_file(&repo, "src/c.ts", "export const c = 1;\n").unwrap();
    write_file(&repo, "tests/b.test.ts", "import { b } from '../src/b';\n").unwrap();
    write_file(&repo, "tests/c.test.ts", "import { c } from '../src/c';\n").unwrap();
    let base = commit_all(&repo, "base").unwrap();
    write_file(&repo, "src/a.ts", "export const a = 2;\n").unwrap();
    let head = commit_all(&repo, "head").unwrap();

    let dataset = serde_json::json!({
        "version": 1,
        "repo": repo.to_string_lossy(),
        "description": "unit test history",
        "cases": [{
            "id": "c1", "base": base, "head": head,
            // b.test really depends on the change; c.test is (pretend) flaky; the .spec.js file
            // is not something the tool recognises as a test.
            "failing_after_change": ["tests/b.test.ts", "tests/c.test.ts", "tests/legacy.spec.js"]
        }]
    });
    let path = tmp.path().join("dataset.json");
    fs::write(&path, serde_json::to_string(&dataset).unwrap()).unwrap();

    let r = historical::evaluate(&path).unwrap();
    let c = &r.cases[0];
    assert_eq!(
        (c.changed_files, c.recommended, c.total_test_files),
        (1, 1, 2)
    );
    assert_eq!(c.reduction, Some(0.5));
    assert_eq!((c.failing, c.found, c.found_known), (3, 1, 1));
    assert_eq!(c.missed, vec!["tests/c.test.ts", "tests/legacy.spec.js"]);
    assert_eq!(c.missed_unrecognized, vec!["tests/legacy.spec.js"]);
    assert_eq!((r.failing_total, r.found_total), (3, 1));
    assert!((r.recall.unwrap() - 1.0 / 3.0).abs() < 1e-9);
    assert!(r.recall_ci95.is_some());
    assert_eq!((r.cases_with_failures, r.cases_fully_covered), (1, 0));
    assert!(!r.caveats.is_empty());

    // The temporary worktree must not be left behind.
    let worktrees = git(&repo, &["worktree", "list"]).unwrap();
    assert_eq!(worktrees.lines().count(), 1, "{worktrees}");
}

#[test]
fn unsupported_dataset_versions_and_missing_commits_are_errors() {
    let tmp = tempfile::TempDir::new().unwrap();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    init_repo(&repo).unwrap();
    write_file(&repo, "src/a.ts", "export const a = 1;\n").unwrap();
    commit_all(&repo, "only").unwrap();

    let v2 = tmp.path().join("v2.json");
    fs::write(
        &v2,
        format!(
            r#"{{"version":2,"repo":{:?},"cases":[]}}"#,
            repo.to_string_lossy()
        ),
    )
    .unwrap();
    assert!(historical::evaluate(&v2).is_err());

    let bad = tmp.path().join("bad.json");
    fs::write(
        &bad,
        format!(
            r#"{{"version":1,"repo":{:?},"cases":[{{"id":"x","base":"deadbeef","head":"cafebabe"}}]}}"#,
            repo.to_string_lossy()
        ),
    )
    .unwrap();
    let err = historical::evaluate(&bad).unwrap_err();
    assert!(
        format!("{err:#}").contains("could not check out"),
        "{err:#}"
    );
}
