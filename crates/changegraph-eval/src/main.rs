//! `changegraph-eval`: reproducible evaluation of ChangeGraph.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use changegraph_eval::bench::{default_bin, run_bench, BenchSpec};
use changegraph_eval::synthetic::SyntheticSpec;
use changegraph_eval::{envelope, historical, render, scenario, synthetic_eval};
use clap::{Parser, Subcommand};
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "changegraph-eval",
    version,
    about = "Measure ChangeGraph's accuracy, test-selection quality, runtime and memory",
    after_help = "Every command can write a machine-readable result file with --json FILE. The file records \
the OS, CPU count and build mode so that numbers can be traced to their conditions. Nothing is printed \
as a result unless it was measured by this run."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Labeled end-to-end scenarios on the fixture projects (hand-written labels).
    Scenarios {
        /// Directory of scenario files.
        #[arg(long, default_value = "eval/scenarios")]
        dir: PathBuf,
        /// Repository root that scenario fixtures are relative to.
        #[arg(long, default_value = ".")]
        repo_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
    /// Precision/recall on generated repositories with exact ground truth.
    Synthetic {
        #[arg(long, value_delimiter = ',', default_value = "100,1000")]
        sizes: Vec<usize>,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
    /// Runtime and peak memory of the real CLI on generated repositories of several sizes.
    Bench {
        #[arg(long, value_delimiter = ',', default_value = "100,1000,5000")]
        sizes: Vec<usize>,
        /// Runs per size (the first is not discarded; min/median/max are all reported).
        #[arg(long, default_value_t = 5)]
        runs: usize,
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Path of the `changegraph` binary (default: next to this executable).
        #[arg(long, value_name = "PATH")]
        changegraph_bin: Option<PathBuf>,
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
    /// Test-selection recall and reduction against recorded outcomes of real commits.
    Historical {
        /// Dataset file (see eval/historical/README.md).
        dataset: PathBuf,
        #[arg(long, value_name = "FILE")]
        json: Option<PathBuf>,
    },
}

fn write_json<T: Serialize>(path: &Option<PathBuf>, command: &str, results: &T) -> Result<()> {
    if let Some(p) = path {
        let file = File::create(p).with_context(|| format!("creating {}", p.display()))?;
        serde_json::to_writer_pretty(BufWriter::new(file), &envelope(command, results))?;
        eprintln!("wrote {}", p.display());
    }
    Ok(())
}

fn warn_if_debug() {
    if cfg!(debug_assertions) {
        eprintln!("note: this harness was built without optimisations; timing and memory figures are not meaningful.");
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Scenarios {
            dir,
            repo_root,
            json,
        } => {
            let scenarios = scenario::load_scenarios(&dir)?;
            let mut results = Vec::new();
            for s in &scenarios {
                results.push(scenario::run_scenario(s, &repo_root)?);
            }
            let agg = scenario::aggregate(&results);
            print!("{}", render::scenarios(&results, &agg));
            #[derive(Serialize)]
            struct Out<'a> {
                scenarios: &'a [scenario::ScenarioResult],
                aggregate: &'a scenario::ScenarioAggregate,
            }
            write_json(
                &json,
                "scenarios",
                &Out {
                    scenarios: &results,
                    aggregate: &agg,
                },
            )
        }
        Command::Synthetic { sizes, seed, json } => {
            let mut results = Vec::new();
            for size in sizes {
                results.push(synthetic_eval::evaluate(&SyntheticSpec::new(size, seed))?);
            }
            print!("{}", render::synthetic(&results));
            write_json(&json, "synthetic", &results)
        }
        Command::Bench {
            sizes,
            runs,
            seed,
            changegraph_bin,
            json,
        } => {
            warn_if_debug();
            let bin = changegraph_bin.unwrap_or_else(default_bin);
            if !bin.to_string_lossy().contains("release") {
                eprintln!(
                    "note: {} is not under a `release` directory; make sure it is an optimised build.",
                    bin.display()
                );
            }
            let results = run_bench(&BenchSpec {
                sizes,
                runs,
                seed,
                bin,
            })?;
            print!("{}", render::bench(&results));
            write_json(&json, "bench", &results)
        }
        Command::Historical { dataset, json } => {
            let result = historical::evaluate(Path::new(&dataset))?;
            print!("{}", render::historical(&result));
            write_json(&json, "historical", &result)
        }
    }
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
