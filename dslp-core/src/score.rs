use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A 0-100 score split into lenient and strict variants plus per-detector
/// subtotals.
///
/// - `lenient` counts only Open issues. Suppressing (wontfix) an issue raises
///   it — that's the "feel good" number an agent can drive toward 100.
/// - `strict` counts Open **and** Wontfix issues, so dismissing an issue does
///   not improve it.
/// - `gap()` = `lenient - strict` (always ≥ 0). A large gap means the lenient
///   number is inflated by dismissals — the anti-gaming signal.
///
/// `by_detector` breaks down the Open-issue penalty per detector id, for a
/// future `--detail` view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub lenient: f64,
    pub strict: f64,
    pub total_loc: u64,
    pub open_count: usize,
    pub wontfix_count: usize,
    pub by_detector: BTreeMap<String, DetectorBreakdown>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectorBreakdown {
    pub findings: usize,
    pub penalty: u32,
}

impl Score {
    pub fn perfect() -> Self {
        Self {
            lenient: 100.0,
            strict: 100.0,
            total_loc: 0,
            open_count: 0,
            wontfix_count: 0,
            by_detector: BTreeMap::new(),
        }
    }

    /// How much the lenient number is inflated by dismissed (wontfix) issues.
    pub fn gap(&self) -> f64 {
        self.lenient - self.strict
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_is_100_with_zero_gap() {
        let p = Score::perfect();
        assert_eq!(p.lenient, 100.0);
        assert_eq!(p.strict, 100.0);
        assert_eq!(p.gap(), 0.0);
        assert!(p.by_detector.is_empty());
    }

    #[test]
    fn score_serde_roundtrips() {
        let mut s = Score::perfect();
        s.lenient = 80.0;
        s.strict = 62.0;
        s.total_loc = 1000;
        s.open_count = 3;
        s.wontfix_count = 2;
        s.by_detector.insert(
            "rust.file-size".into(),
            DetectorBreakdown {
                findings: 2,
                penalty: 6,
            },
        );
        let j = serde_json::to_string(&s).unwrap();
        let r: Score = serde_json::from_str(&j).unwrap();
        assert_eq!(s, r);
        assert_eq!(r.gap(), 18.0);
    }
}
