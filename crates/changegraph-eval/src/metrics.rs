//! Metric definitions. Everything here is plain arithmetic on counts so that it can be unit
//! tested against independently computed values.

use std::collections::BTreeSet;

use serde::Serialize;

/// True positives, false positives and false negatives of a predicted set against a truth set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub tp: usize,
    pub fp: usize,
    pub fn_: usize,
}

impl Counts {
    pub fn add(&mut self, other: &Counts) {
        self.tp += other.tp;
        self.fp += other.fp;
        self.fn_ += other.fn_;
    }

    /// `None` when nothing was predicted (precision is undefined, not 100%).
    pub fn precision(&self) -> Option<f64> {
        let d = self.tp + self.fp;
        (d > 0).then(|| self.tp as f64 / d as f64)
    }

    /// `None` when the truth set is empty (recall is undefined, not 100%).
    pub fn recall(&self) -> Option<f64> {
        let d = self.tp + self.fn_;
        (d > 0).then(|| self.tp as f64 / d as f64)
    }

    pub fn f1(&self) -> Option<f64> {
        let (p, r) = (self.precision()?, self.recall()?);
        (p + r > 0.0).then(|| 2.0 * p * r / (p + r))
    }

    /// True if prediction and truth agree exactly (including both being empty).
    pub fn is_exact(&self) -> bool {
        self.fp == 0 && self.fn_ == 0
    }
}

pub fn compare<T: Ord>(predicted: &BTreeSet<T>, truth: &BTreeSet<T>) -> Counts {
    let tp = predicted.intersection(truth).count();
    Counts {
        tp,
        fp: predicted.len() - tp,
        fn_: truth.len() - tp,
    }
}

/// Serializable form of [`Counts`] with the derived metrics.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Score {
    pub tp: usize,
    pub fp: usize,
    #[serde(rename = "fn")]
    pub fn_: usize,
    pub precision: Option<f64>,
    pub recall: Option<f64>,
    pub f1: Option<f64>,
}

impl From<Counts> for Score {
    fn from(c: Counts) -> Self {
        Self {
            tp: c.tp,
            fp: c.fp,
            fn_: c.fn_,
            precision: c.precision(),
            recall: c.recall(),
            f1: c.f1(),
        }
    }
}

impl Score {
    pub fn counts(&self) -> Counts {
        Counts {
            tp: self.tp,
            fp: self.fp,
            fn_: self.fn_,
        }
    }
}

/// 95% Wilson score interval for a proportion `successes / n`. `None` if `n == 0`.
///
/// Used for small samples (e.g. "found 7 of 8 failing tests"), where a bare percentage would
/// overstate certainty.
pub fn wilson_interval(successes: usize, n: usize) -> Option<(f64, f64)> {
    if n == 0 {
        return None;
    }
    let z = 1.959_963_984_540_054_f64;
    let n_f = n as f64;
    let p = successes as f64 / n_f;
    let z2 = z * z;
    let denom = 1.0 + z2 / n_f;
    let center = (p + z2 / (2.0 * n_f)) / denom;
    let half = z * (p * (1.0 - p) / n_f + z2 / (4.0 * n_f * n_f)).sqrt() / denom;
    Some(((center - half).max(0.0), (center + half).min(1.0)))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Spread {
    pub n: usize,
    pub min: f64,
    pub median: f64,
    pub max: f64,
    pub mean: f64,
}

pub fn spread(values: &[f64]) -> Option<Spread> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    let median = if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    };
    Some(Spread {
        n,
        min: v[0],
        median,
        max: v[n - 1],
        mean: v.iter().sum::<f64>() / n as f64,
    })
}

/// Fraction of the full suite that is *not* run: `1 - recommended / total`. `None` if `total == 0`.
pub fn reduction(recommended: usize, total: usize) -> Option<f64> {
    (total > 0).then(|| 1.0 - recommended as f64 / total as f64)
}

pub fn fmt_opt(v: Option<f64>) -> String {
    v.map_or_else(|| "n/a".to_string(), |x| format!("{x:.3}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn compares_sets() {
        let c = compare(&set(&["a", "b", "c"]), &set(&["b", "c", "d", "e"]));
        assert_eq!(
            c,
            Counts {
                tp: 2,
                fp: 1,
                fn_: 2
            }
        );
        assert!(close(c.precision().unwrap(), 2.0 / 3.0));
        assert!(close(c.recall().unwrap(), 0.5));
        assert!(close(c.f1().unwrap(), 4.0 / 7.0));
        assert!(!c.is_exact());
    }

    #[test]
    fn empty_cases_are_undefined_not_perfect() {
        let both_empty = compare(&set(&[]), &set(&[]));
        assert_eq!(both_empty.precision(), None);
        assert_eq!(both_empty.recall(), None);
        assert!(both_empty.is_exact());

        let nothing_predicted = compare(&set(&[]), &set(&["a"]));
        assert_eq!(nothing_predicted.precision(), None);
        assert_eq!(nothing_predicted.recall(), Some(0.0));
        assert_eq!(nothing_predicted.f1(), None);

        let nothing_true = compare(&set(&["a"]), &set(&[]));
        assert_eq!(nothing_true.precision(), Some(0.0));
        assert_eq!(nothing_true.recall(), None);
    }

    #[test]
    fn counts_add_for_micro_averaging() {
        let mut total = Counts::default();
        total.add(&Counts {
            tp: 3,
            fp: 1,
            fn_: 0,
        });
        total.add(&Counts {
            tp: 1,
            fp: 0,
            fn_: 2,
        });
        assert_eq!(
            total,
            Counts {
                tp: 4,
                fp: 1,
                fn_: 2
            }
        );
    }

    #[test]
    fn wilson_matches_reference_values() {
        // Reference values computed independently (see docs/evaluation.md).
        let (lo, hi) = wilson_interval(8, 10).unwrap();
        assert!(close(lo, 0.4902) && close(hi, 0.9433), "{lo} {hi}");
        let (lo, hi) = wilson_interval(10, 10).unwrap();
        assert!(close(lo, 0.7225) && close(hi, 1.0), "{lo} {hi}");
        let (lo, hi) = wilson_interval(0, 5).unwrap();
        assert!(close(lo, 0.0) && close(hi, 0.4345), "{lo} {hi}");
        let (lo, hi) = wilson_interval(1, 1).unwrap();
        assert!(close(lo, 0.2065) && close(hi, 1.0), "{lo} {hi}");
        assert_eq!(wilson_interval(0, 0), None);
    }

    #[test]
    fn spread_handles_odd_even_and_empty() {
        let s = spread(&[3.0, 1.0, 2.0]).unwrap();
        assert_eq!((s.min, s.median, s.max, s.n), (1.0, 2.0, 3.0, 3));
        assert!(close(s.mean, 2.0));
        assert_eq!(spread(&[4.0, 1.0, 3.0, 2.0]).unwrap().median, 2.5);
        assert!(spread(&[]).is_none());
    }

    #[test]
    fn reduction_is_the_share_of_tests_not_run() {
        assert!(close(reduction(2, 10).unwrap(), 0.8));
        assert_eq!(reduction(0, 0), None);
        assert!(close(reduction(10, 10).unwrap(), 0.0));
    }
}
