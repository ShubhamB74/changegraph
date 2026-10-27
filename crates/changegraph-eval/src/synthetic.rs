//! Deterministic synthetic repositories **with exact ground truth by construction**.
//!
//! The generator decides every import, so the true dependency graph is known without reading
//! the files back. That allows precision/recall of dependency extraction and of impact analysis
//! to be measured at any size. Limits: the repositories are synthetic (clean syntax, a DAG, one
//! import style); they validate that the implementation is *correct on well-formed input*, not
//! that it generalises to real code. Real-world behaviour is what `historical` is for.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use anyhow::Result;

use crate::gitutil::write_file;

/// Small PCG-style generator so datasets are reproducible from a seed on every platform.
pub struct Lcg(u64);

impl Lcg {
    pub fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u31(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    /// Uniform in `[0, 1)`.
    pub fn f64(&mut self) -> f64 {
        self.next_u31() as f64 / 2_147_483_648.0
    }

    /// Uniform in `[0, n)`; `n` must be > 0.
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u31() as usize) % n
    }
}

#[derive(Debug, Clone)]
pub struct SyntheticSpec {
    pub files: usize,
    pub seed: u64,
    pub test_fraction: f64,
    pub imports_per_module: usize,
    pub unresolved_rate: f64,
    pub external_rate: f64,
}

impl SyntheticSpec {
    pub fn new(files: usize, seed: u64) -> Self {
        Self {
            files,
            seed,
            test_fraction: 0.2,
            imports_per_module: 3,
            unresolved_rate: 0.02,
            external_rate: 0.05,
        }
    }
}

/// What the generator wrote, i.e. the correct answer.
#[derive(Debug, Clone)]
pub struct Truth {
    pub modules: Vec<String>,
    pub tests: Vec<String>,
    /// `(importer, imported)`.
    pub edges: BTreeSet<(String, String)>,
    /// `(importer, package)`.
    pub external: BTreeSet<(String, String)>,
    /// `(importer, specifier as written)`.
    pub unresolved: BTreeSet<(String, String)>,
}

/// Relative module specifier from `from_file` to `to_file` (no extension), e.g. `./m1` or
/// `../../src/d000/m5`.
pub fn relative_specifier(from_file: &str, to_file: &str) -> String {
    let from_dir: Vec<&str> = {
        let mut parts: Vec<&str> = from_file.split('/').collect();
        parts.pop();
        parts
    };
    let to_no_ext = to_file.strip_suffix(".ts").unwrap_or(to_file);
    let to_parts: Vec<&str> = to_no_ext.split('/').collect();
    let to_dir = &to_parts[..to_parts.len() - 1];

    let common = from_dir
        .iter()
        .zip(to_dir.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let ups = from_dir.len() - common;
    let mut out = if ups == 0 {
        "./".to_string()
    } else {
        "../".repeat(ups)
    };
    out.push_str(&to_parts[common..].join("/"));
    out
}

pub fn module_path(i: usize) -> String {
    format!("src/d{:03}/m{:05}.ts", i / 50, i)
}

pub fn test_path(j: usize) -> String {
    format!("tests/d{:03}/t{:05}.test.ts", j / 50, j)
}

pub fn generate(dir: &Path, spec: &SyntheticSpec) -> Result<Truth> {
    let n_tests = ((spec.files as f64) * spec.test_fraction).round() as usize;
    let n_modules = spec.files.saturating_sub(n_tests).max(1);
    let mut rng = Lcg::new(spec.seed);

    let modules: Vec<String> = (0..n_modules).map(module_path).collect();
    let tests: Vec<String> = (0..n_tests).map(test_path).collect();
    let mut edges = BTreeSet::new();
    let mut external = BTreeSet::new();
    let mut unresolved = BTreeSet::new();

    for (i, path) in modules.iter().enumerate() {
        let mut targets = BTreeSet::new();
        if i > 0 {
            for _ in 0..spec.imports_per_module.min(i) {
                // Squared uniform skews toward low indices, producing a few heavily imported hubs.
                let u = rng.f64();
                let j = ((i as f64) * u * u) as usize;
                targets.insert(j.min(i - 1));
            }
        }
        let mut body = String::new();
        for (k, j) in targets.iter().enumerate() {
            let spec_str = relative_specifier(path, &modules[*j]);
            body.push_str(&format!("import {{ v{j} as a{k} }} from \"{spec_str}\";\n"));
            edges.insert((path.clone(), modules[*j].clone()));
        }
        if rng.f64() < spec.unresolved_rate {
            let spec_str = format!("./missing_{i}");
            body.push_str(&format!("import \"{spec_str}\";\n"));
            unresolved.insert((path.clone(), spec_str));
        }
        if rng.f64() < spec.external_rate {
            let pkg = format!("ext-pkg-{}", i % 7);
            body.push_str(&format!("import ext from \"{pkg}\";\n"));
            external.insert((path.clone(), pkg));
        }
        body.push_str(&format!("export const v{i} = {i};\n"));
        write_file(dir, path, &body)?;
    }

    for (t, path) in tests.iter().enumerate() {
        let mut targets = BTreeSet::new();
        for _ in 0..(1 + rng.below(3)) {
            targets.insert(rng.below(n_modules));
        }
        let mut body = String::new();
        for (k, j) in targets.iter().enumerate() {
            let spec_str = relative_specifier(path, &modules[*j]);
            body.push_str(&format!("import {{ v{j} as a{k} }} from \"{spec_str}\";\n"));
            edges.insert((path.clone(), modules[*j].clone()));
        }
        body.push_str(&format!("test(\"t{t}\", () => {{}});\n"));
        write_file(dir, path, &body)?;
    }

    Ok(Truth {
        modules,
        tests,
        edges,
        external,
        unresolved,
    })
}

/// Reverse reachability on the true edges: every file that imports (transitively) one of
/// `changed`, with its shortest distance. Written independently of the engine on purpose.
pub fn truth_impact(edges: &BTreeSet<(String, String)>, changed: &[String]) -> BTreeMap<String, usize> {
    let mut importers: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (from, to) in edges {
        importers.entry(to.as_str()).or_default().push(from.as_str());
    }
    let mut dist: BTreeMap<String, usize> = changed.iter().map(|c| (c.clone(), 0)).collect();
    let mut queue: VecDeque<String> = changed.iter().cloned().collect();
    while let Some(v) = queue.pop_front() {
        let d = dist[&v];
        for &w in importers.get(v.as_str()).map(Vec::as_slice).unwrap_or(&[]) {
            if !dist.contains_key(w) {
                dist.insert(w.to_string(), d + 1);
                queue.push_back(w.to_string());
            }
        }
    }
    for c in changed {
        dist.remove(c);
    }
    dist
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_specifiers() {
        assert_eq!(relative_specifier("src/d000/m00005.ts", "src/d000/m00002.ts"), "./m00002");
        assert_eq!(relative_specifier("src/d001/m00060.ts", "src/d000/m00002.ts"), "../d000/m00002");
        assert_eq!(
            relative_specifier("tests/d000/t00001.test.ts", "src/d002/m00123.ts"),
            "../../src/d002/m00123"
        );
        assert_eq!(relative_specifier("src/d003/m1.ts", "src/d003/m1.ts"), "./m1");
    }

    #[test]
    fn paths_are_stable_and_bucketed() {
        assert_eq!(module_path(0), "src/d000/m00000.ts");
        assert_eq!(module_path(49), "src/d000/m00049.ts");
        assert_eq!(module_path(50), "src/d001/m00050.ts");
        assert_eq!(test_path(7), "tests/d000/t00007.test.ts");
    }

    #[test]
    fn rng_is_reproducible_and_in_range() {
        let mut a = Lcg::new(7);
        let mut b = Lcg::new(7);
        let xs: Vec<u64> = (0..5).map(|_| a.next_u31()).collect();
        let ys: Vec<u64> = (0..5).map(|_| b.next_u31()).collect();
        assert_eq!(xs, ys);
        let mut r = Lcg::new(1);
        for _ in 0..1000 {
            let f = r.f64();
            assert!((0.0..1.0).contains(&f));
            assert!(r.below(13) < 13);
        }
        assert_ne!(Lcg::new(1).next_u31(), Lcg::new(2).next_u31());
    }

    #[test]
    fn truth_impact_is_reverse_reachability_with_shortest_distance() {
        let e = |a: &str, b: &str| (a.to_string(), b.to_string());
        // c -> b -> a, d -> a, d -> c
        let edges: BTreeSet<_> = [e("c", "b"), e("b", "a"), e("d", "a"), e("d", "c")].into_iter().collect();
        let got = truth_impact(&edges, &["a".to_string()]);
        let expected: BTreeMap<String, usize> =
            [("b", 1), ("d", 1), ("c", 2)].into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn generation_is_deterministic_and_the_truth_is_a_dag() {
        let spec = SyntheticSpec::new(120, 11);
        let d1 = tempfile::TempDir::new().unwrap();
        let d2 = tempfile::TempDir::new().unwrap();
        let t1 = generate(d1.path(), &spec).unwrap();
        let t2 = generate(d2.path(), &spec).unwrap();
        assert_eq!(t1.edges, t2.edges);
        assert_eq!(t1.modules.len() + t1.tests.len(), 120);
        for p in t1.modules.iter().chain(t1.tests.iter()) {
            assert_eq!(
                std::fs::read_to_string(d1.path().join(p)).unwrap(),
                std::fs::read_to_string(d2.path().join(p)).unwrap()
            );
        }
        // Modules only import lower-numbered modules, so there are no cycles by construction.
        let index = |p: &str| t1.modules.iter().position(|m| m == p);
        for (from, to) in &t1.edges {
            if let (Some(a), Some(b)) = (index(from), index(to)) {
                assert!(b < a);
            }
        }
        assert!(!t1.edges.is_empty());
    }
}
