//! Scorecard badge — a shields.io "endpoint badge" payload
//! (<https://shields.io/badges/endpoint-badge>) for embedding a live score
//! badge in a README, e.g. via
//! `https://img.shields.io/endpoint?url=<hosted badge.json>`.
//!
//! Defaults to the *strict* score: unlike lenient, it can't be inflated by
//! suppressing issues, so it's the honest number to expose externally.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Badge {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub label: String,
    pub message: String,
    pub color: &'static str,
}

impl Badge {
    /// Build a badge payload from a 0-100 score.
    pub fn from_score(score: f64, label: &str) -> Self {
        Badge {
            schema_version: 1,
            label: label.to_string(),
            message: format!("{:.0}", score),
            color: color_for(score),
        }
    }

    /// Placeholder badge for a project that hasn't been scanned yet.
    pub fn no_scan(label: &str) -> Self {
        Badge {
            schema_version: 1,
            label: label.to_string(),
            message: "no scan".to_string(),
            color: "lightgrey",
        }
    }
}

fn color_for(score: f64) -> &'static str {
    match score {
        s if s >= 90.0 => "brightgreen",
        s if s >= 75.0 => "green",
        s if s >= 50.0 => "yellow",
        s if s >= 25.0 => "orange",
        _ => "red",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_thresholds() {
        assert_eq!(color_for(100.0), "brightgreen");
        assert_eq!(color_for(90.0), "brightgreen");
        assert_eq!(color_for(89.9), "green");
        assert_eq!(color_for(75.0), "green");
        assert_eq!(color_for(74.9), "yellow");
        assert_eq!(color_for(50.0), "yellow");
        assert_eq!(color_for(49.9), "orange");
        assert_eq!(color_for(25.0), "orange");
        assert_eq!(color_for(24.9), "red");
        assert_eq!(color_for(0.0), "red");
    }

    #[test]
    fn from_score_rounds_message() {
        let b = Badge::from_score(87.6, "dslp");
        assert_eq!(b.message, "88");
        assert_eq!(b.schema_version, 1);
        assert_eq!(b.label, "dslp");
    }

    #[test]
    fn no_scan_badge_is_lightgrey() {
        let b = Badge::no_scan("dslp");
        assert_eq!(b.color, "lightgrey");
        assert_eq!(b.message, "no scan");
    }

    #[test]
    fn serializes_with_shields_schema_field_name() {
        let b = Badge::from_score(90.0, "dslp");
        let j = serde_json::to_string(&b).unwrap();
        assert!(j.contains("\"schemaVersion\":1"), "got: {}", j);
    }
}
