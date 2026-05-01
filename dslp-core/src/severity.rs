use serde::{Deserialize, Serialize};

/// How impactful a finding is. Weight feeds the scoring formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    /// Score weight per the design doc (§3).
    pub fn weight(self) -> u32 {
        match self {
            Severity::Low => 1,
            Severity::Medium => 3,
            Severity::High => 8,
            Severity::Critical => 20,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_are_strictly_increasing() {
        let order = [
            Severity::Low,
            Severity::Medium,
            Severity::High,
            Severity::Critical,
        ];
        for w in order.windows(2) {
            assert!(w[0].weight() < w[1].weight(), "{:?} < {:?}", w[0], w[1]);
        }
    }

    #[test]
    fn weight_values_match_design() {
        assert_eq!(Severity::Low.weight(), 1);
        assert_eq!(Severity::Medium.weight(), 3);
        assert_eq!(Severity::High.weight(), 8);
        assert_eq!(Severity::Critical.weight(), 20);
    }

    #[test]
    fn severity_serializes_lowercase() {
        let json = serde_json::to_string(&Severity::High).unwrap();
        assert_eq!(json, r#""high""#);
    }
}
