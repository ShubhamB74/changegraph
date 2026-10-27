//! Runtime and memory benchmarks of the real `changegraph` binary on synthetic repositories.
//!
//! Each run is a separate process. Wall time is measured around the process; peak resident
//! memory comes from the kernel's per-child accounting (`wait4`), so it reflects exactly that
//! process (including its `git` subprocesses' parent only, not the harness).
//!
//! Numbers are only meaningful for a **release** build of the CLI on an otherwise idle machine.

use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use changegraph_core::Report;
use serde::Serialize;

use crate::gitutil::{commit_all, init_repo};
use crate::metrics::{spread, Spread};
use crate::synthetic::{generate, SyntheticSpec};

#[derive(Debug, Clone)]
pub struct Measured {
    pub wall_ms: f64,
    pub max_rss_kb: Option<u64>,
    pub exit_code: i32,
}

/// Run `cmd` to completion, discarding its output, and measure it.
#[cfg(unix)]
#[allow(unknown_lints, clippy::zombie_processes)] // the child is reaped by wait4 below
pub fn run_measured(cmd: &mut Command) -> Result<Measured> {
    use std::process::Stdio;
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let start = Instant::now();
    let child = cmd.spawn().context("failed to start the command")?;
    let pid = child.id() as libc::pid_t;
    let mut status: libc::c_int = 0;
    // SAFETY: `rusage` is plain old data; zero-initialised is a valid value.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: `pid` is our direct child and both out-pointers are valid for the call.
    let rc = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    if rc < 0 {
        bail!("wait4 failed: {}", std::io::Error::last_os_error());
    }
    let exit_code = if libc::WIFEXITED(status) {
        libc::WEXITSTATUS(status)
    } else {
        -1
    };
    // ru_maxrss is in kilobytes on Linux and in bytes on macOS.
    let raw = usage.ru_maxrss as u64;
    let kb = if cfg!(target_os = "macos") {
        raw / 1024
    } else {
        raw
    };
    drop(child);
    Ok(Measured {
        wall_ms,
        max_rss_kb: Some(kb),
        exit_code,
    })
}

/// Non-Unix fallback: wall time only (peak memory is reported as unavailable, never guessed).
#[cfg(not(unix))]
pub fn run_measured(cmd: &mut Command) -> Result<Measured> {
    use std::process::Stdio;
    cmd.stdout(Stdio::null()).stderr(Stdio::null());
    let start = Instant::now();
    let status = cmd.status().context("failed to run the command")?;
    Ok(Measured {
        wall_ms: start.elapsed().as_secs_f64() * 1000.0,
        max_rss_kb: None,
        exit_code: status.code().unwrap_or(-1),
    })
}

#[derive(Debug, Clone)]
pub struct BenchSpec {
    pub sizes: Vec<usize>,
    pub runs: usize,
    pub seed: u64,
    pub bin: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct BenchResult {
    pub files: usize,
    pub runs: usize,
    pub wall_ms: Option<Spread>,
    /// Highest peak RSS over the runs, in MiB. `None` where the platform gives no figure.
    pub max_rss_mib: Option<f64>,
    /// Time the engine reports for itself in the last run's report.
    pub engine_ms: Option<u64>,
    pub report_bytes: u64,
    pub graph_files: usize,
    pub graph_edges: usize,
    pub parse_clean_share: Option<f64>,
    pub unresolved_share: Option<f64>,
    pub changed_files: usize,
    pub affected: usize,
}

pub fn default_bin() -> PathBuf {
    let name = if cfg!(windows) {
        "changegraph.exe"
    } else {
        "changegraph"
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(name)))
        .unwrap_or_else(|| PathBuf::from(name))
}

pub fn run_bench(spec: &BenchSpec) -> Result<Vec<BenchResult>> {
    if !spec.bin.is_file() {
        bail!(
            "changegraph binary not found at {}. Build both tools in release mode first:\n  \
             cargo build --release -p changegraph-cli -p changegraph-eval\n\
             or pass --changegraph-bin <path>.",
            spec.bin.display()
        );
    }
    if spec.runs == 0 {
        bail!("--runs must be at least 1");
    }
    let mut out = Vec::new();
    for &size in &spec.sizes {
        let repo = tempfile::TempDir::new()?;
        let outdir = tempfile::TempDir::new()?;
        let report_path = outdir.path().join("report.json");

        let truth = generate(repo.path(), &SyntheticSpec::new(size, spec.seed))?;
        init_repo(repo.path())?;
        commit_all(repo.path(), "synthetic")?;
        // Change the most-imported module: the worst case for impact analysis.
        let hub = repo.path().join(&truth.modules[0]);
        let mut text = fs::read_to_string(&hub)?;
        text.push_str("// edit\n");
        fs::write(&hub, text)?;

        let mut walls = Vec::new();
        let mut rss_kb: Option<u64> = None;
        for _ in 0..spec.runs {
            let mut cmd = Command::new(&spec.bin);
            cmd.arg("analyze")
                .arg(repo.path())
                .args(["--format", "json", "--output"])
                .arg(&report_path);
            let m = run_measured(&mut cmd)?;
            if m.exit_code != 0 {
                bail!(
                    "`changegraph analyze` exited with code {} on a {size}-file repository",
                    m.exit_code
                );
            }
            walls.push(m.wall_ms);
            rss_kb = match (rss_kb, m.max_rss_kb) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (None, b) => b,
                (a, None) => a,
            };
        }
        let report = read_report(&report_path)?;
        let g = &report.graph.stats;
        let imports = g.edges + g.external_dependencies + g.unresolved_imports;
        out.push(BenchResult {
            files: size,
            runs: spec.runs,
            wall_ms: spread(&walls),
            max_rss_mib: rss_kb.map(|kb| kb as f64 / 1024.0),
            engine_ms: report.execution.duration_ms,
            report_bytes: fs::metadata(&report_path)?.len(),
            graph_files: g.files,
            graph_edges: g.edges,
            parse_clean_share: (g.files > 0).then(|| g.parsed_ok as f64 / g.files as f64),
            unresolved_share: (imports > 0).then(|| g.unresolved_imports as f64 / imports as f64),
            changed_files: report.impact.summary.changed_files,
            affected: report.impact.summary.affected,
        });
    }
    Ok(out)
}

fn read_report(path: &Path) -> Result<Report> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .context("the report written by the CLI could not be parsed")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn measures_exit_code_time_and_memory_of_a_child() {
        let m = run_measured(Command::new("sh").args(["-c", "exit 3"])).unwrap();
        assert_eq!(m.exit_code, 3);
        assert!(m.wall_ms >= 0.0 && m.wall_ms < 10_000.0);
        assert!(
            m.max_rss_kb.unwrap() > 0,
            "a started process has a non-zero peak RSS"
        );

        let ok = run_measured(Command::new("sh").args(["-c", "true"])).unwrap();
        assert_eq!(ok.exit_code, 0);
    }

    #[test]
    fn a_missing_binary_is_an_error_not_a_zero() {
        assert!(run_measured(&mut Command::new("/definitely/not/a/binary")).is_err());
    }
}
