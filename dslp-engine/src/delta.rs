//! What a scan changed relative to the previous state, for the post-scan
//! summary and CI gating.

use std::collections::HashMap;

use dslp_core::{Issue, Status};
use serde::Serialize;

use crate::state::State;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScanDelta {
    /// Open issues whose fingerprint was not in the previous state.
    pub new: usize,
    /// Open before, no longer detected.
    pub fixed: usize,
    /// Resolved before, detected again.
    pub regressed: usize,
    /// Strict score before this scan; `None` on the first scan.
    pub previous_strict: Option<f64>,
}

impl ScanDelta {
    /// Whether there was a previous scan to compare against.
    pub fn has_baseline(&self) -> bool {
        self.previous_strict.is_some()
    }
}

pub fn delta(previous: Option<&State>, issues: &[Issue]) -> ScanDelta {
    let before: HashMap<&str, &Status> = previous
        .map(|s| s.issues.iter().map(|i| (i.fingerprint.as_str(), &i.status)).collect())
        .unwrap_or_default();
    let mut d = ScanDelta {
        new: 0,
        fixed: 0,
        regressed: 0,
        previous_strict: previous.map(|s| s.score.strict),
    };
    for issue in issues {
        match (before.get(issue.fingerprint.as_str()), &issue.status) {
            (None, Status::Open) => d.new += 1,
            (Some(Status::Open), Status::Resolved) => d.fixed += 1,
            (Some(Status::Resolved), Status::Open) => d.regressed += 1,
            _ => {}
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{merge_issues, STATE_VERSION};
    use chrono::{DateTime, Utc};
    use dslp_core::{Finding, Score, Severity};
    use std::path::PathBuf;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-01T20:00:00Z").unwrap().with_timezone(&Utc)
    }

    fn finding(snippet: &str) -> Finding {
        Finding::new("d", Severity::Low, PathBuf::from("a.rs"), None, "m", None, snippet)
    }

    fn state(issues: Vec<Issue>) -> State {
        State { version: STATE_VERSION, last_scan: now(), score: Score::perfect(), issues }
    }

    #[test]
    fn first_scan_has_no_baseline() {
        let issues = merge_issues(None, vec![finding("a")], now());
        let d = delta(None, &issues);
        assert_eq!((d.new, d.fixed, d.regressed), (1, 0, 0));
        assert!(!d.has_baseline());
    }

    #[test]
    fn counts_new_fixed_and_regressed() {
        let s0 = state(merge_issues(None, vec![finding("keep"), finding("gone"), finding("back")], now()));
        let mut s1 = state(merge_issues(Some(&s0), vec![finding("keep"), finding("back")], now()));
        s1.issues.iter_mut().find(|i| i.fingerprint == finding("back").fingerprint).unwrap().resolve();
        let next = merge_issues(Some(&s1), vec![finding("keep"), finding("back"), finding("fresh")], now());
        assert_eq!(delta(Some(&s1), &next), ScanDelta { new: 1, fixed: 0, regressed: 1, previous_strict: Some(100.0) });
        let after_gone = merge_issues(Some(&s0), vec![finding("keep"), finding("back")], now());
        let d = delta(Some(&s0), &after_gone);
        assert_eq!((d.new, d.fixed, d.regressed), (0, 1, 0));
        assert!(d.has_baseline());
    }
}
