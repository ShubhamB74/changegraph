use std::path::Path;

use changegraph_core::discovery::{discover_sources, ExcludeSet, FileCategory};

fn paths(files: &[changegraph_core::discovery::SourceFile]) -> Vec<&str> {
    files.iter().map(|f| f.path.as_str()).collect()
}

#[test]
fn discovers_fixture_project_files_sorted_and_classified() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/simple-ts-project");
    let (files, warnings) = discover_sources(&root, &ExcludeSet::new(&[], true));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(
        paths(&files),
        vec![
            "src/api/profile.ts",
            "src/auth/session.ts",
            "src/index.ts",
            "tests/profile.test.ts",
            "tests/session.test.ts",
        ]
    );
    let tests = files.iter().filter(|f| f.category == FileCategory::Test).count();
    assert_eq!(tests, 2);
}

#[test]
fn skips_default_excludes_gitignored_and_non_source_files() {
    let dir = tempfile::TempDir::new().unwrap();
    let w = |rel: &str| {
        let p = dir.path().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x\n").unwrap();
    };
    w("src/a.ts");
    w("src/legacy/old.ts");
    w("node_modules/pkg/index.js");
    w("dist/bundle.js");
    w("generated/gen.ts");
    w("README.md");
    w(".gitignore");
    std::fs::write(dir.path().join(".gitignore"), "generated/\n").unwrap();

    let (files, _) = discover_sources(dir.path(), &ExcludeSet::new(&[], true));
    assert_eq!(paths(&files), vec!["src/a.ts", "src/legacy/old.ts"]);

    let (files, _) = discover_sources(
        dir.path(),
        &ExcludeSet::new(&["src/legacy".to_string()], true),
    );
    assert_eq!(paths(&files), vec!["src/a.ts"]);

    // With defaults disabled, node_modules and dist are visible again.
    let (files, _) = discover_sources(dir.path(), &ExcludeSet::new(&[], false));
    assert!(paths(&files).contains(&"node_modules/pkg/index.js"));
    assert!(paths(&files).contains(&"dist/bundle.js"));
    assert!(!paths(&files).contains(&"generated/gen.ts"));
}
