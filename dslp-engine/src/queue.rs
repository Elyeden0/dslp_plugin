use dslp_core::Issue;

use crate::state::State;

/// Open issues ranked highest-priority first.
///
/// Priority = severity weight. Ties broken by `first_seen` ascending so older
/// items aren't starved by newer arrivals of the same severity. Only `Open`
/// issues appear — Resolved and Wontfix are excluded.
pub fn queue(state: &State) -> Vec<&Issue> {
    let mut v: Vec<&Issue> = state.issues.iter().filter(|i| i.is_open()).collect();
    v.sort_by(|a, b| {
        let aw = a.severity.weight();
        let bw = b.severity.weight();
        bw.cmp(&aw).then_with(|| a.first_seen.cmp(&b.first_seen))
    });
    v
}

/// The single highest-priority open issue, if any.
pub fn next_issue(state: &State) -> Option<&Issue> {
    queue(state).into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use dslp_core::{Finding, Score, Severity, Span};
    use std::path::PathBuf;

    use crate::state::{merge_issues, State, STATE_VERSION};

    fn dt(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn finding(sev: Severity, file: &str, snippet: &str) -> Finding {
        Finding::new(
            "d",
            sev,
            PathBuf::from(file),
            Some(Span::single_line(1, 0, 0)),
            "msg",
            None,
            snippet,
        )
    }

    fn state_with(issues: Vec<dslp_core::Issue>) -> State {
        State {
            version: STATE_VERSION,
            last_scan: dt("2026-05-01T20:00:00Z"),
            score: Score::perfect(),
            issues,
        }
    }

    #[test]
    fn higher_severity_first() {
        let issues = merge_issues(
            None,
            vec![
                finding(Severity::Low, "a.rs", "a"),
                finding(Severity::High, "b.rs", "b"),
            ],
            dt("2026-05-01T20:00:00Z"),
        );
        let state = state_with(issues);
        let n = next_issue(&state).unwrap();
        assert_eq!(n.severity, Severity::High);
    }

    #[test]
    fn wontfix_and_resolved_excluded() {
        let mut issues = merge_issues(
            None,
            vec![
                finding(Severity::High, "a.rs", "a"),
                finding(Severity::High, "b.rs", "b"),
            ],
            dt("2026-05-01T20:00:00Z"),
        );
        issues[0].suppress("x", dt("2026-05-01T20:00:00Z"));
        issues[1].resolve();
        let state = state_with(issues);
        assert!(next_issue(&state).is_none());
    }
}
