use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Finding, Severity, Span};

/// Lifecycle state of an [`Issue`].
///
/// - `Open` — actionable; counts against the score and appears in `next`.
/// - `Resolved` — optimistically marked fixed (or confirmed gone by a scan).
///   If a later scan re-detects the fingerprint, it reopens as a regression.
/// - `Wontfix` — a standing decision to not fix, with a required reason. It
///   survives rescans and is excluded from `next`, but it still counts toward
///   the *strict* score so dismissing issues can't game the headline number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Status {
    Open,
    Resolved,
    Wontfix { reason: String, at: DateTime<Utc> },
}

impl Status {
    pub fn is_open(&self) -> bool {
        matches!(self, Status::Open)
    }
    pub fn is_wontfix(&self) -> bool {
        matches!(self, Status::Wontfix { .. })
    }
    pub fn is_resolved(&self) -> bool {
        matches!(self, Status::Resolved)
    }
}

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
    pub status: Status,
}

impl Issue {
    /// Build a fresh Issue from a newly-discovered Finding. Starts `Open`.
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
            status: Status::Open,
        }
    }

    /// Update an existing Issue with data from a re-detected Finding.
    ///
    /// `first_seen` is preserved. Status transition on re-detection:
    /// - `Wontfix` is preserved (a standing decision survives rescans).
    /// - Anything else becomes `Open` — including a `Resolved` issue that has
    ///   come back, which is a regression and should be surfaced again.
    pub fn refresh(&mut self, finding: Finding, now: DateTime<Utc>) {
        self.severity = finding.severity;
        self.span = finding.span;
        self.message = finding.message;
        self.fix_hint = finding.fix_hint;
        self.last_seen = now;
        if !self.status.is_wontfix() {
            self.status = Status::Open;
        }
    }

    pub fn is_open(&self) -> bool {
        self.status.is_open()
    }

    pub fn resolve(&mut self) {
        self.status = Status::Resolved;
    }

    /// Put a Resolved or Wontfix issue back in the queue.
    pub fn reopen(&mut self) {
        self.status = Status::Open;
    }

    pub fn suppress(&mut self, reason: impl Into<String>, at: DateTime<Utc>) {
        self.status = Status::Wontfix {
            reason: reason.into(),
            at,
        };
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
    fn from_new_finding_starts_open() {
        let f = finding("hi", Severity::Low);
        let i = Issue::from_new_finding(f.clone(), now());
        assert_eq!(i.first_seen, now());
        assert_eq!(i.last_seen, now());
        assert!(i.is_open());
        assert_eq!(i.fingerprint, f.fingerprint);
    }

    #[test]
    fn refresh_reopens_resolved_as_regression() {
        let mut i = Issue::from_new_finding(finding("old", Severity::Low), now());
        i.resolve();
        assert!(i.status.is_resolved());
        i.refresh(finding("back", Severity::High), later());
        assert!(i.is_open(), "re-detected resolved issue should reopen");
        assert_eq!(i.first_seen, now());
        assert_eq!(i.last_seen, later());
        assert_eq!(i.severity, Severity::High);
    }

    #[test]
    fn refresh_preserves_wontfix() {
        let mut i = Issue::from_new_finding(finding("x", Severity::Low), now());
        i.suppress("acceptable here", now());
        i.refresh(finding("x", Severity::Low), later());
        assert!(i.status.is_wontfix(), "wontfix should survive a rescan");
        assert_eq!(i.last_seen, later());
    }

    #[test]
    fn reopen_clears_wontfix() {
        let mut i = Issue::from_new_finding(finding("x", Severity::Low), now());
        i.suppress("meh", now());
        i.reopen();
        assert!(i.is_open());
    }

    #[test]
    fn status_serde_roundtrips_wontfix() {
        let s = Status::Wontfix {
            reason: "intentional".into(),
            at: now(),
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("wontfix"));
        let r: Status = serde_json::from_str(&j).unwrap();
        assert_eq!(s, r);
    }
}
