//! Batch triage: group open issues by file so an agent can fix a whole file
//! in one pass instead of working the flat priority queue one issue at a
//! time. `queue`/`next` already rank individual issues; `plan` ranks *files*.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use dslp_core::{Issue, Severity};
use serde::Serialize;

use crate::state::State;

/// One file's worth of open issues, batched for triage.
#[derive(Debug, Clone, Serialize)]
pub struct PlanEntry {
    pub file: PathBuf,
    /// Sum of severity weights across the file's open issues.
    pub penalty: u32,
    pub issue_count: usize,
    pub highest_severity: Severity,
    /// Earliest `first_seen` among the file's open issues.
    pub oldest: DateTime<Utc>,
}

/// Open issues grouped by file, ranked by total penalty descending — the
/// file at the top yields the biggest score jump per fix. Ties broken by
/// `oldest` ascending, so long-standing backlogs aren't starved by newer
/// arrivals of equal weight.
pub fn plan(state: &State) -> Vec<PlanEntry> {
    let mut groups: BTreeMap<&PathBuf, Vec<&Issue>> = BTreeMap::new();
    for issue in state.issues.iter().filter(|i| i.is_open()) {
        groups.entry(&issue.file).or_default().push(issue);
    }

    let mut entries: Vec<PlanEntry> = groups
        .into_iter()
        .map(|(file, issues)| PlanEntry {
            file: file.clone(),
            penalty: issues.iter().map(|i| i.severity.weight()).sum(),
            issue_count: issues.len(),
            highest_severity: issues
                .iter()
                .map(|i| i.severity)
                .max_by_key(|s| s.weight())
                .expect("group is never empty"),
            oldest: issues
                .iter()
                .map(|i| i.first_seen)
                .min()
                .expect("group is never empty"),
        })
        .collect();

    entries.sort_by(|a, b| b.penalty.cmp(&a.penalty).then_with(|| a.oldest.cmp(&b.oldest)));
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use dslp_core::{Finding, Score, Span};
    use std::path::PathBuf as StdPathBuf;

    use crate::state::{merge_issues, STATE_VERSION};

    fn dt(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn finding(sev: Severity, file: &str, snippet: &str) -> Finding {
        Finding::new(
            "d",
            sev,
            StdPathBuf::from(file),
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
    fn empty_state_has_no_plan() {
        assert!(plan(&state_with(vec![])).is_empty());
    }

    #[test]
    fn groups_by_file_and_sums_penalty() {
        let issues = merge_issues(
            None,
            vec![
                finding(Severity::Low, "a.rs", "a1"),
                finding(Severity::Medium, "a.rs", "a2"),
                finding(Severity::Low, "b.rs", "b1"),
            ],
            dt("2026-05-01T20:00:00Z"),
        );
        let entries = plan(&state_with(issues));
        assert_eq!(entries.len(), 2);
        let a = entries.iter().find(|e| e.file == PathBuf::from("a.rs")).unwrap();
        assert_eq!(a.penalty, Severity::Low.weight() + Severity::Medium.weight());
        assert_eq!(a.issue_count, 2);
        assert_eq!(a.highest_severity, Severity::Medium);
    }

    #[test]
    fn higher_penalty_file_ranks_first() {
        let issues = merge_issues(
            None,
            vec![
                finding(Severity::Low, "quiet.rs", "q1"),
                finding(Severity::Critical, "loud.rs", "l1"),
            ],
            dt("2026-05-01T20:00:00Z"),
        );
        let entries = plan(&state_with(issues));
        assert_eq!(entries[0].file, PathBuf::from("loud.rs"));
    }

    #[test]
    fn ties_broken_by_oldest_first() {
        let older = merge_issues(None, vec![finding(Severity::Medium, "old.rs", "o1")], dt("2026-01-01T00:00:00Z"));
        let mut state = state_with(older);
        let newer = merge_issues(
            Some(&state),
            vec![
                finding(Severity::Medium, "old.rs", "o1"),
                finding(Severity::Medium, "new.rs", "n1"),
            ],
            dt("2026-06-01T00:00:00Z"),
        );
        state.issues = newer;
        let entries = plan(&state);
        assert_eq!(entries[0].penalty, entries[1].penalty, "both files have equal penalty");
        assert_eq!(entries[0].file, PathBuf::from("old.rs"), "older file should rank first on a tie");
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
        assert!(plan(&state_with(issues)).is_empty());
    }
}
