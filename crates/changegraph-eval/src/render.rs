//! Plain-text tables. Undefined metrics print as `n/a`, never as 0 or 1.

use crate::bench::BenchResult;
use crate::historical::HistoricalResult;
use crate::metrics::{fmt_opt, Score};
use crate::scenario::{ScenarioAggregate, ScenarioResult};
use crate::synthetic_eval::SyntheticResult;

fn pr(s: &Score) -> String {
    format!("P={} R={} (tp={} fp={} fn={})", fmt_opt(s.precision), fmt_opt(s.recall), s.tp, s.fp, s.fn_)
}

pub fn scenarios(results: &[ScenarioResult], agg: &ScenarioAggregate) -> String {
    let mut out = String::new();
    out.push_str("LABELED SCENARIOS (hand-written labels; small projects)\n\n");
    for r in results {
        out.push_str(&format!("* {}\n", r.name));
        out.push_str(&format!("    affected      {}\n", pr(&r.affected)));
        if !r.missing_affected.is_empty() {
            out.push_str(&format!("      missing:    {}\n", r.missing_affected.join(", ")));
        }
        if !r.unexpected_affected.is_empty() {
            out.push_str(&format!("      unexpected: {}\n", r.unexpected_affected.join(", ")));
        }
        out.push_str(&format!("    tests (known) {}\n", pr(&r.tests_known)));
        out.push_str(&format!("    tests (all)   {}\n", pr(&r.tests_all)));
        if let Some(u) = &r.uncertain {
            out.push_str(&format!("    uncertain     {}\n", pr(u)));
        }
        out.push_str(&format!(
            "    recommended {} of {} test files (reduction {}), selection complete: {}\n",
            r.recommended,
            r.total_test_files,
            fmt_opt(r.reduction),
            r.selection_complete
        ));
        out.push_str(&format!(
            "    parsed cleanly {}, unresolved import share {}\n",
            fmt_opt(r.parse_clean_share),
            fmt_opt(r.unresolved_share)
        ));
    }
    out.push_str(&format!(
        "\nAGGREGATE over {} scenarios (micro-averaged)\n    affected      {}\n    tests (known) {}\n    tests (all)   {}\n    exact affected sets: {}/{}, exact known-test sets: {}/{}\n    mean reduction: {}\n",
        agg.scenarios,
        pr(&agg.affected),
        pr(&agg.tests_known),
        pr(&agg.tests_all),
        agg.exact_affected,
        agg.scenarios,
        agg.exact_tests_known,
        agg.scenarios,
        fmt_opt(agg.mean_reduction)
    ));
    out.push_str("\nThese compare the tool with a human reading of small projects. They do not measure regression-catching power.\n");
    out
}

pub fn synthetic(results: &[SyntheticResult]) -> String {
    let mut out = String::from("SYNTHETIC REPOSITORIES (exact ground truth by construction; clean syntax, DAG imports)\n");
    for r in results {
        out.push_str(&format!(
            "\n{} files (seed {}): {} modules, {} tests, {} true edges, parsed cleanly {}\n",
            r.files, r.seed, r.modules, r.tests, r.true_edges, fmt_opt(r.parse_clean_share)
        ));
        out.push_str(&format!("    extraction edges      {}\n", pr(&r.extraction_edges)));
        out.push_str(&format!("    extraction external   {}\n", pr(&r.extraction_external)));
        out.push_str(&format!("    extraction unresolved {}\n", pr(&r.extraction_unresolved)));
        for t in &r.targets {
            out.push_str(&format!(
                "    change {}: truth affected {}, impact {}, distances exact {}/{}, tests(known) {}, reduction {}, engine {} ms\n",
                t.target,
                t.truth_affected,
                pr(&t.impact),
                t.distance_exact,
                t.truth_affected,
                pr(&t.tests_known),
                fmt_opt(t.reduction),
                t.engine_ms.map_or("n/a".to_string(), |m| m.to_string())
            ));
        }
    }
    out.push_str("\nPerfect scores here show correctness on well-formed input; they say nothing about real code.\n");
    out
}

pub fn bench(results: &[BenchResult]) -> String {
    let mut out = String::from(
        "BENCHMARK (real CLI binary, synthetic repos; meaningful only for release builds on an idle machine)\n\n",
    );
    out.push_str("files  graph files  edges   wall ms (min / median / max)   peak RSS MiB   engine ms   report MiB   affected\n");
    for r in results {
        let wall = r.wall_ms.as_ref().map_or("n/a".to_string(), |s| {
            format!("{:.0} / {:.0} / {:.0}", s.min, s.median, s.max)
        });
        out.push_str(&format!(
            "{:<6} {:<12} {:<7} {:<30} {:<14} {:<11} {:<12.2} {}\n",
            r.files,
            r.graph_files,
            r.graph_edges,
            wall,
            r.max_rss_mib.map_or("n/a".to_string(), |m| format!("{m:.1}")),
            r.engine_ms.map_or("n/a".to_string(), |m| m.to_string()),
            r.report_bytes as f64 / (1024.0 * 1024.0),
            r.affected
        ));
    }
    out
}

pub fn historical(r: &HistoricalResult) -> String {
    let mut out = String::from("HISTORICAL OUTCOMES (recorded test results of real commits)\n");
    if !r.dataset_description.is_empty() {
        out.push_str(&format!("dataset: {}\n", r.dataset_description));
    }
    out.push('\n');
    for c in &r.cases {
        out.push_str(&format!(
            "* {}: recommended {} of {} test files (reduction {}), failing {}, found {} (known {}), selection complete: {}\n",
            c.id, c.recommended, c.total_test_files, fmt_opt(c.reduction), c.failing, c.found, c.found_known, c.selection_complete
        ));
        for m in &c.missed {
            let why = if c.missed_unrecognized.contains(m) { " (not recognised as a test file by the tool)" } else { "" };
            out.push_str(&format!("      MISSED {m}{why}\n"));
        }
    }
    let ci = r
        .recall_ci95
        .map_or("n/a".to_string(), |(lo, hi)| format!("{lo:.3}..{hi:.3}"));
    out.push_str(&format!(
        "\nAGGREGATE: {} of {} newly failing test files recommended; recall {} (95% Wilson interval {}); known-basis only {}\n    cases with failures fully covered: {}/{}; mean reduction {} (only complete selections: {})\n",
        r.found_total,
        r.failing_total,
        fmt_opt(r.recall),
        ci,
        fmt_opt(r.recall_known_only),
        r.cases_fully_covered,
        r.cases_with_failures,
        fmt_opt(r.mean_reduction),
        fmt_opt(r.mean_reduction_when_complete)
    ));
    out.push_str("\nCAVEATS\n");
    for c in &r.caveats {
        out.push_str(&format!("  - {c}\n"));
    }
    out
}
