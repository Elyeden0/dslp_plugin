use std::collections::BTreeMap;

use dslp_core::{DetectorBreakdown, Issue, Score, Status};

/// Map a total penalty + project size to a 0-100 composite, per design §3:
/// `100 * exp(-penalty / (size_kloc * 25))`.
fn composite(penalty: u32, total_loc: u64) -> f64 {
    let size = ((total_loc / 1000).max(1)) as f64;
    100.0 * (-(penalty as f64) / (size * 25.0)).exp()
}

/// Score a reconciled issue list into lenient + strict variants.
///
/// - lenient penalty = Open issues only
/// - strict penalty  = Open + Wontfix issues
/// - Resolved issues count toward neither
///
/// Because strict penalty ≥ lenient penalty, `strict ≤ lenient` always, so
/// `gap = lenient - strict ≥ 0`.
pub fn score_issues(issues: &[Issue], total_loc: u64) -> Score {
    let mut lenient_penalty: u32 = 0;
    let mut strict_penalty: u32 = 0;
    let mut open_count = 0usize;
    let mut wontfix_count = 0usize;
    let mut by_detector: BTreeMap<String, DetectorBreakdown> = BTreeMap::new();

    for issue in issues {
        let w = issue.severity.weight();
        match issue.status {
            Status::Open => {
                lenient_penalty += w;
                strict_penalty += w;
                open_count += 1;
                let entry = by_detector
                    .entry(issue.detector.clone())
                    .or_insert(DetectorBreakdown {
                        findings: 0,
                        penalty: 0,
                    });
                entry.findings += 1;
                entry.penalty += w;
            }
            Status::Wontfix { .. } => {
                strict_penalty += w;
                wontfix_count += 1;
            }
            Status::Resolved => {}
        }
    }

    Score {
        lenient: composite(lenient_penalty, total_loc),
        strict: composite(strict_penalty, total_loc),
        total_loc,
        open_count,
        wontfix_count,
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
    fn no_issues_yields_100_both() {
        let s = score_issues(&[], 1000);
        assert!((s.lenient - 100.0).abs() < 1e-9);
        assert!((s.strict - 100.0).abs() < 1e-9);
        assert_eq!(s.gap(), 0.0);
    }

    #[test]
    fn all_open_means_lenient_equals_strict() {
        let issues = vec![
            open_issue("d", Severity::High, "a"),
            open_issue("d", Severity::Medium, "b"),
        ];
        let s = score_issues(&issues, 1000);
        assert!((s.lenient - s.strict).abs() < 1e-9, "no wontfix → no gap");
        assert_eq!(s.open_count, 2);
        assert_eq!(s.wontfix_count, 0);
    }

    #[test]
    fn suppressing_raises_lenient_leaves_strict_and_widens_gap() {
        let mut issues = vec![
            open_issue("d", Severity::High, "a"),
            open_issue("d", Severity::High, "b"),
        ];
        let before = score_issues(&issues, 1000);
        // Suppress one of the two.
        issues[0].suppress("acceptable", now());
        let after = score_issues(&issues, 1000);

        assert!(after.lenient > before.lenient, "lenient improves when an open issue is dismissed");
        assert!((after.strict - before.strict).abs() < 1e-9, "strict unchanged by dismissal");
        assert!(after.gap() > before.gap(), "gap widens");
        assert_eq!(after.open_count, 1);
        assert_eq!(after.wontfix_count, 1);
    }

    #[test]
    fn resolving_improves_both() {
        let mut issues = vec![
            open_issue("d", Severity::High, "a"),
            open_issue("d", Severity::High, "b"),
        ];
        let before = score_issues(&issues, 1000);
        issues[0].resolve();
        let after = score_issues(&issues, 1000);
        assert!(after.lenient > before.lenient);
        assert!(after.strict > before.strict, "resolving an issue helps strict too");
    }

    #[test]
    fn lenient_never_below_strict() {
        let mut issues = vec![open_issue("d", Severity::Critical, "a")];
        issues[0].suppress("x", now());
        let s = score_issues(&issues, 1000);
        assert!(s.lenient >= s.strict);
    }
}
