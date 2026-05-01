use std::collections::BTreeMap;

use dslp_core::{DetectorBreakdown, Issue, Score};

/// Map a total penalty + project size to a 0-100 composite, per design §3:
/// `100 * exp(-penalty / (size_kloc * 25))`.
fn composite(penalty: u32, total_loc: u64) -> f64 {
    let size = ((total_loc / 1000).max(1)) as f64;
    100.0 * (-(penalty as f64) / (size * 25.0)).exp()
}

/// Score a reconciled issue list. Only open (unresolved) issues contribute
/// penalty.
pub fn score_issues(issues: &[Issue], total_loc: u64) -> Score {
    let mut penalty: u32 = 0;
    let mut finding_count = 0usize;
    let mut by_detector: BTreeMap<String, DetectorBreakdown> = BTreeMap::new();

    for issue in issues.iter().filter(|i| i.is_open()) {
        let w = issue.severity.weight();
        penalty += w;
        finding_count += 1;
        let entry = by_detector
            .entry(issue.detector.clone())
            .or_insert(DetectorBreakdown {
                findings: 0,
                penalty: 0,
            });
        entry.findings += 1;
        entry.penalty += w;
    }

    Score {
        composite: composite(penalty, total_loc),
        total_loc,
        finding_count,
        by_detector,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use dslp_core::{Finding, Severity, Span};
    use std::path::PathBuf;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-01T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn open_issue(detector: &str, sev: Severity, snippet: &str) -> Issue {
        let f = Finding::new(
            detector,
            sev,
            PathBuf::from("a.rs"),
            Some(Span::single_line(1, 0, 0)),
            "msg",
            None,
            snippet,
        );
        Issue::from_new_finding(f, now())
    }

    #[test]
    fn no_findings_yields_100() {
        let s = score_issues(&[], 1000);
        assert!((s.composite - 100.0).abs() < 1e-9);
    }

    #[test]
    fn higher_severity_penalizes_more() {
        let low = score_issues(&[open_issue("d", Severity::Low, "a")], 1000);
        let high = score_issues(&[open_issue("d", Severity::High, "a")], 1000);
        assert!(high.composite < low.composite);
    }

    #[test]
    fn resolved_issues_excluded() {
        let mut issues = vec![open_issue("d", Severity::High, "a")];
        issues[0].resolved = true;
        let s = score_issues(&issues, 1000);
        assert!((s.composite - 100.0).abs() < 1e-9);
    }

    #[test]
    fn larger_codebase_softens_penalty() {
        let issue = open_issue("d", Severity::High, "a");
        let small = score_issues(&[issue.clone()], 500);
        let large = score_issues(&[issue], 50_000);
        assert!(large.composite > small.composite);
    }

    #[test]
    fn breakdown_separates_detectors() {
        let issues = vec![
            open_issue("rust.file-size", Severity::Medium, "a"),
            open_issue("rust.complexity", Severity::High, "b"),
        ];
        let s = score_issues(&issues, 1000);
        assert_eq!(s.by_detector.len(), 2);
        assert_eq!(s.by_detector["rust.file-size"].findings, 1);
    }
}
