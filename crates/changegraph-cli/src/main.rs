//! `changegraph` command-line interface.
//!
//! Exit codes: 0 success; 1 internal/I-O error; 2 usage or invalid configuration;
//! 3 path/repository problem; 4 revision problem; 5 git failure; 6 analysis succeeded but a
//! `--fail-on` condition was met (the report is still written).
//! (Argument-parsing errors are reported by clap with exit code 2.)

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use changegraph_core::report::{write_json_with, write_text};
use changegraph_core::{analyze, AnalysisConfig, ChangeGraphError, Report};
use clap::{Args, Parser, Subcommand, ValueEnum};

/// Exit code used when `--fail-on` triggers.
const EXIT_FAIL_ON: u8 = 6;

#[derive(Parser)]
#[command(
    name = "changegraph",
    version,
    about = "Local-first change impact analysis for TypeScript/JavaScript repositories",
    after_help = "EXIT CODES:\n  0  success\n  1  internal or I/O error\n  2  usage error or invalid configuration (including an invalid --coverage-map)\n  3  path does not exist, is not a directory, or is not a Git repository\n  4  unknown or invalid revision, or no merge base\n  5  git missing or failed\n  6  analysis succeeded but a --fail-on condition was met (report still written)"
)]
struct Cli {
    /// Increase log verbosity on stderr (-v = debug, -vv = trace).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Detect changed files, build the dependency graph, compute change impact, and
    /// recommend tests.
    ///
    /// With --base, compares committed changes on the target (default HEAD) against
    /// its merge base with BASE (like `git diff BASE...TARGET`). Uncommitted work is
    /// not included in that mode; a warning is emitted if any exists.
    ///
    /// Without --base, reports uncommitted changes (staged, unstaged and untracked)
    /// relative to HEAD. The two modes are never mixed.
    Analyze(AnalyzeArgs),
}

#[derive(Clone, Copy, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FailOn {
    /// Never fail because of analysis results (default).
    None,
    /// Exit 6 if the report contains any warning.
    Warnings,
    /// Exit 6 if test selection is incomplete (see `test_recommendations.completeness`).
    IncompleteTests,
}

#[derive(Args)]
struct AnalyzeArgs {
    /// Path to the repository (or any directory inside it).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Base revision (branch, tag, or commit). There is no default branch.
    #[arg(long)]
    base: Option<String>,

    /// Target revision to compare against the base. Requires --base. [default: HEAD]
    #[arg(long, requires = "base")]
    target: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    /// Emit JSON without whitespace (smaller for large repositories). Only with --format json.
    #[arg(long)]
    compact: bool,

    /// Exclude a directory/file name (e.g. `dist`) or relative path prefix
    /// (e.g. `src/legacy`). May be repeated.
    #[arg(short = 'x', long, value_name = "PATTERN")]
    exclude: Vec<String>,

    /// Do not apply the built-in exclusions (node_modules, .git, dist, build, out,
    /// coverage, .next, .turbo).
    #[arg(long)]
    no_default_excludes: bool,

    /// Maximum dependent distance to follow when computing impact (default: unlimited).
    /// If the limit cuts off further dependents, the report says so.
    #[arg(long, value_name = "N")]
    max_depth: Option<usize>,

    /// Per-test coverage map (JSON) used as an additional, optional source of test
    /// relationships: {"version": 1, "tests": {"<test path>": ["<source path>", ...]}}.
    #[arg(long, value_name = "FILE")]
    coverage_map: Option<PathBuf>,

    /// Omit wall-clock timing (`execution.duration_ms` becomes null) so that identical
    /// repository state and options give a byte-identical report.
    #[arg(long)]
    no_timing: bool,

    /// Exit with code 6 (after writing the report) when the chosen condition is met.
    #[arg(long, value_enum, default_value_t = FailOn::None)]
    fail_on: FailOn,

    /// Write the report to FILE instead of stdout. The file is written atomically and is
    /// only created if the analysis succeeds.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match run(&cli) {
        Ok(None) => ExitCode::SUCCESS,
        Ok(Some(reason)) => {
            eprintln!("fail-on condition met: {reason}");
            ExitCode::from(EXIT_FAIL_ON)
        }
        Err(err) => {
            eprintln!("error: {err}");
            if let Some(hint) = err.hint() {
                eprintln!("hint: {hint}");
            }
            ExitCode::from(err.exit_code())
        }
    }
}

fn init_tracing(verbose: u8) {
    use tracing_subscriber::EnvFilter;
    let default = match verbose {
        0 => "warn",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    // Ignore the error if a global subscriber was already installed.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(io::stderr)
        .try_init();
}

/// Returns `Ok(Some(reason))` if a `--fail-on` condition was met.
fn run(cli: &Cli) -> Result<Option<String>, ChangeGraphError> {
    match &cli.command {
        Command::Analyze(args) => run_analyze(args),
    }
}

fn run_analyze(args: &AnalyzeArgs) -> Result<Option<String>, ChangeGraphError> {
    let mut config = AnalysisConfig::new(&args.path);
    config.base = args.base.clone();
    config.target = args.target.clone();
    config.exclude = args.exclude.clone();
    config.use_default_excludes = !args.no_default_excludes;
    config.max_depth = args.max_depth;
    config.coverage_map = args.coverage_map.clone();
    config.record_timing = !args.no_timing;

    if args.compact && !matches!(args.format, OutputFormat::Json) {
        return Err(ChangeGraphError::InvalidConfig(
            "--compact only applies to --format json".to_string(),
        ));
    }

    let report = analyze(&config)?;

    // The report is serialized straight into the destination; it is never rendered into an
    // intermediate String.
    match &args.output {
        Some(path) => write_atomic(path, args, &report)?,
        None => {
            let mut out = BufWriter::new(io::stdout().lock());
            write_report(args, &report, &mut out)?;
            out.flush().map_err(ChangeGraphError::Output)?;
        }
    }

    Ok(fail_on_reason(args.fail_on, &report))
}

fn fail_on_reason(fail_on: FailOn, report: &Report) -> Option<String> {
    match fail_on {
        FailOn::None => None,
        FailOn::Warnings => (!report.warnings.is_empty())
            .then(|| format!("the report contains {} warning(s)", report.warnings.len())),
        FailOn::IncompleteTests => (!report.test_recommendations.completeness.complete)
            .then(|| "test selection is incomplete".to_string()),
    }
}

fn write_report<W: Write>(
    args: &AnalyzeArgs,
    report: &Report,
    writer: W,
) -> Result<(), ChangeGraphError> {
    match args.format {
        OutputFormat::Json => write_json_with(report, writer, !args.compact),
        OutputFormat::Text => write_text(report, writer),
    }
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> ChangeGraphError {
    let path = path.to_path_buf();
    move |source| ChangeGraphError::Io { path, source }
}

/// Write to a temporary file in the destination directory, then rename over `path`, so a
/// failure never leaves a truncated report behind.
fn write_atomic(path: &Path, args: &AnalyzeArgs, report: &Report) -> Result<(), ChangeGraphError> {
    let mut tmp_name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    tmp_name.push(format!(".tmp-{}", std::process::id()));
    let tmp = path.with_file_name(tmp_name);

    let result = (|| -> Result<(), ChangeGraphError> {
        let file = File::create(&tmp).map_err(io_error(&tmp))?;
        let mut writer = BufWriter::new(file);
        write_report(args, report, &mut writer)?;
        writer.flush().map_err(ChangeGraphError::Output)?;
        Ok(())
    })();

    match result {
        Ok(()) => fs::rename(&tmp, path).map_err(|source| {
            let _ = fs::remove_file(&tmp);
            ChangeGraphError::Io {
                path: path.to_path_buf(),
                source,
            }
        }),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}
