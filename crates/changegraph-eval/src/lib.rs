//! Evaluation and benchmark harness for ChangeGraph.
//!
//! Nothing in this crate produces a number unless it measured it. See `docs/evaluation.md` for
//! definitions, methodology and the limits of each experiment.

pub mod bench;
pub mod gitutil;
pub mod historical;
pub mod metrics;
pub mod render;
pub mod scenario;
pub mod synthetic;
pub mod synthetic_eval;

use serde::Serialize;

/// Provenance recorded with every result file so that a number can be traced to its conditions.
#[derive(Debug, Serialize)]
pub struct Envelope<'a, T: Serialize> {
    pub harness_version: &'a str,
    pub command: &'a str,
    pub os: &'a str,
    pub arch: &'a str,
    pub cpus: usize,
    /// False means the harness itself was built without optimisations; timings are then suspect.
    pub harness_release_build: bool,
    pub results: &'a T,
}

pub fn envelope<'a, T: Serialize>(command: &'a str, results: &'a T) -> Envelope<'a, T> {
    Envelope {
        harness_version: env!("CARGO_PKG_VERSION"),
        command,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        cpus: std::thread::available_parallelism().map_or(1, |n| n.get()),
        harness_release_build: !cfg!(debug_assertions),
        results,
    }
}
