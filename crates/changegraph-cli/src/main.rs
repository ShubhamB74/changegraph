//! `changegraph` command-line interface.
//!
//! Exit codes: 0 success, 1 internal/I-O error, 2 usage or invalid configuration,
//! 3 path/repository problem, 4 revision problem, 5 git failure.
//! (Argument-parsing errors are reported by clap with exit code 2.)

use std::fs::File;
use std::io::{self, BufWriter};
use std::path::PathBuf;
use std::process::ExitCode;

use changegraph_core::report::{write_json, write_text};
use changegraph_core::{analyze, AnalysisConfig, ChangeGraphError};
use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "changegraph",
    version,
    about = "Local-first change impact analysis for TypeScript/JavaScript repositories"
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
    /// Detect changed files, build the dependency graph, and compute change impact.
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

    /// Write the report to FILE instead of stdout.
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
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

fn run(cli: &Cli) -> Result<(), ChangeGraphError> {
    match &cli.command {
        Command::Analyze(args) => run_analyze(args),
    }
}

fn run_analyze(args: &AnalyzeArgs) -> Result<(), ChangeGraphError> {
    let mut config = AnalysisConfig::new(&args.path);
    config.base = args.base.clone();
    config.target = args.target.clone();
    config.exclude = args.exclude.clone();
    config.use_default_excludes = !args.no_default_excludes;
    config.max_depth = args.max_depth;

    let report = analyze(&config)?;

    // Stream straight to the destination; the report is never rendered into an
    // intermediate String.
    match &args.output {
        Some(path) => {
            let file = File::create(path).map_err(|source| ChangeGraphError::Io {
                path: path.clone(),
                source,
            })?;
            write_report(args.format, &report, BufWriter::new(file))
        }
        None => write_report(args.format, &report, BufWriter::new(io::stdout().lock())),
    }
}

fn write_report<W: io::Write>(
    format: OutputFormat,
    report: &changegraph_core::Report,
    writer: W,
) -> Result<(), ChangeGraphError> {
    match format {
        OutputFormat::Json => write_json(report, writer),
        OutputFormat::Text => write_text(report, writer),
    }
}
