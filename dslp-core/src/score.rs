use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A 0-100 composite score plus per-detector subtotals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub composite: f64,
    pub total_loc: u64,
    pub finding_count: usize,
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
            composite: 100.0,
            total_loc: 0,
            finding_count: 0,
            by_detector: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_is_100() {
        let p = Score::perfect();
        assert_eq!(p.composite, 100.0);
        assert!(p.by_detector.is_empty());
    }

    #[test]
    fn score_serde_roundtrips() {
        let mut s = Score::perfect();
        s.composite = 80.0;
        s.total_loc = 1000;
        s.finding_count = 3;
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
    }
}
