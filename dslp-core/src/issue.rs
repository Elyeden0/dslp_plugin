use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Finding, Severity, Span};

/// A finding tracked across scans. `first_seen` survives merges; `last_seen`
/// updates every time a scan re-detects the same fingerprint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub fingerprint: String,
    pub detector: String,
    pub severity: Severity,
    pub file: PathBuf,
    pub span: Option<Span>,
    pub message: String,
    pub fix_hint: Option<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub resolved: bool,
}

impl Issue {
    /// Build a fresh Issue from a newly-discovered Finding.
    pub fn from_new_finding(finding: Finding, now: DateTime<Utc>) -> Self {
        Self {
            fingerprint: finding.fingerprint,
            detector: finding.detector,
            severity: finding.severity,
            file: finding.file,
            span: finding.span,
            message: finding.message,
            fix_hint: finding.fix_hint,
            first_seen: now,
            last_seen: now,
            resolved: false,
        }
    }

    /// Update an existing Issue with data from a re-detected Finding.
    /// `first_seen` is preserved; the issue is marked unresolved again.
    pub fn refresh(&mut self, finding: Finding, now: DateTime<Utc>) {
        self.severity = finding.severity;
        self.span = finding.span;
        self.message = finding.message;
        self.fix_hint = finding.fix_hint;
        self.last_seen = now;
        self.resolved = false;
    }

    pub fn is_open(&self) -> bool {
        !self.resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-01T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn later() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-02T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn finding(message: &str, severity: Severity) -> Finding {
        Finding::new(
            "rust.file-size",
            severity,
            PathBuf::from("src/foo.rs"),
            None,
            message,
            None,
            "snippet",
        )
    }

    #[test]
    fn from_new_finding_initializes_timestamps() {
        let f = finding("hi", Severity::Low);
        let i = Issue::from_new_finding(f.clone(), now());
        assert_eq!(i.first_seen, now());
        assert_eq!(i.last_seen, now());
        assert!(i.is_open());
        assert_eq!(i.fingerprint, f.fingerprint);
    }

    #[test]
    fn refresh_preserves_first_seen_and_updates_last() {
        let mut i = Issue::from_new_finding(finding("old", Severity::Low), now());
        i.resolved = true;
        i.refresh(finding("back", Severity::High), later());
        assert!(i.is_open(), "re-detected issue should be unresolved again");
        assert_eq!(i.first_seen, now());
        assert_eq!(i.last_seen, later());
        assert_eq!(i.severity, Severity::High);
    }
}
