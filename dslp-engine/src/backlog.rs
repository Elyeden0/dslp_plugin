//! The wontfix backlog: standing decisions to not fix something. `queue` and
//! `findings` deliberately show only Open issues; this is the dedicated view
//! onto what's been acknowledged and deferred, since otherwise a Wontfix
//! issue is invisible outside of raw `.dslp/state.json`.

use chrono::{DateTime, Utc};
use dslp_core::{Issue, Status};

use crate::state::State;

/// Wontfix issues, most recently suppressed first.
pub fn backlog(state: &State) -> Vec<&Issue> {
    let mut v: Vec<&Issue> = state.issues.iter().filter(|i| i.status.is_wontfix()).collect();
    v.sort_by(|a, b| wontfix_at(b).cmp(&wontfix_at(a)));
    v
}

fn wontfix_at(issue: &Issue) -> DateTime<Utc> {
    match &issue.status {
        Status::Wontfix { at, .. } => *at,
        _ => unreachable!("backlog only contains Wontfix issues"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dslp_core::{Finding, Score, Severity, Span};
    use std::path::PathBuf;

    use crate::state::{merge_issues, STATE_VERSION};

    fn dt(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn finding(file: &str, snippet: &str) -> Finding {
        Finding::new(
            "d",
            Severity::Medium,
            PathBuf::from(file),
            Some(Span::single_line(1, 0, 0)),
            "msg",
            None,
            snippet,
        )
    }

    fn state_with(issues: Vec<Issue>) -> State {
        State {
            version: STATE_VERSION,
            last_scan: dt("2026-05-01T20:00:00Z"),
            score: Score::perfect(),
            issues,
        }
    }

    #[test]
    fn only_wontfix_appears() {
        let mut issues = merge_issues(
            None,
            vec![finding("a.rs", "a"), finding("b.rs", "b"), finding("c.rs", "c")],
            dt("2026-05-01T20:00:00Z"),
        );
        issues[0].suppress("acceptable", dt("2026-05-01T20:00:00Z"));
        issues[1].resolve();
        // issues[2] stays Open.
        let state = state_with(issues);
        let b = backlog(&state);
        assert_eq!(b.len(), 1);
        assert!(b[0].status.is_wontfix());
    }

    #[test]
    fn most_recently_suppressed_first() {
        let mut issues = merge_issues(
            None,
            vec![finding("a.rs", "a"), finding("b.rs", "b")],
            dt("2026-05-01T20:00:00Z"),
        );
        for issue in &mut issues {
            if issue.file == PathBuf::from("a.rs") {
                issue.suppress("older decision", dt("2026-01-01T00:00:00Z"));
            } else {
                issue.suppress("newer decision", dt("2026-06-01T00:00:00Z"));
            }
        }
        let state = state_with(issues);
        let b = backlog(&state);
        assert_eq!(b[0].file, PathBuf::from("b.rs"));
        assert_eq!(b[1].file, PathBuf::from("a.rs"));
    }

    #[test]
    fn empty_when_nothing_suppressed() {
        let issues = merge_issues(None, vec![finding("a.rs", "a")], dt("2026-05-01T20:00:00Z"));
        assert!(backlog(&state_with(issues)).is_empty());
    }
}
