use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use dslp_core::{Finding, Issue, Score};

pub const STATE_VERSION: u32 = 1;

/// On-disk state for a single project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    pub last_scan: DateTime<Utc>,
    pub score: Score,
    pub issues: Vec<Issue>,
}

pub fn state_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".dslp").join("state.json")
}

pub fn load(project_root: &Path) -> anyhow::Result<Option<State>> {
    let path = state_path(project_root);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    let state: State = serde_json::from_str(&text)?;
    Ok(Some(state))
}

pub fn save(project_root: &Path, state: &State) -> anyhow::Result<()> {
    let path = state_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}

/// Reconcile a fresh scan's findings against the previous issue list.
/// A prior issue not re-detected this scan is marked resolved.
pub fn merge_issues(
    previous: Option<&State>,
    findings: Vec<Finding>,
    now: DateTime<Utc>,
) -> Vec<Issue> {
    let mut by_fp: HashMap<String, Issue> = previous
        .map(|s| {
            s.issues
                .iter()
                .cloned()
                .map(|i| (i.fingerprint.clone(), i))
                .collect()
        })
        .unwrap_or_default();

    let mut current_fps = HashSet::new();
    for f in findings {
        current_fps.insert(f.fingerprint.clone());
        match by_fp.remove(&f.fingerprint) {
            Some(mut existing) => {
                existing.refresh(f, now);
                by_fp.insert(existing.fingerprint.clone(), existing);
            }
            None => {
                let new = Issue::from_new_finding(f, now);
                by_fp.insert(new.fingerprint.clone(), new);
            }
        }
    }

    for issue in by_fp.values_mut() {
        if !current_fps.contains(&issue.fingerprint) {
            issue.resolved = true;
        }
    }

    let mut issues: Vec<Issue> = by_fp.into_values().collect();
    issues.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use dslp_core::Severity;
    use std::path::PathBuf;

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

    fn finding(file: &str, snippet: &str) -> Finding {
        Finding::new(
            "d",
            Severity::Medium,
            PathBuf::from(file),
            Some(dslp_core::Span::single_line(1, 0, 0)),
            "msg",
            None,
            snippet,
        )
    }

    fn state_with(issues: Vec<Issue>) -> State {
        State {
            version: STATE_VERSION,
            last_scan: now(),
            score: Score::perfect(),
            issues,
        }
    }

    #[test]
    fn first_scan_creates_issues() {
        let issues = merge_issues(None, vec![finding("a.rs", "x")], now());
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_open());
    }

    #[test]
    fn second_scan_preserves_first_seen_and_updates_last_seen() {
        let f = finding("a.rs", "x");
        let s = state_with(merge_issues(None, vec![f.clone()], now()));
        let issues = merge_issues(Some(&s), vec![f], later());
        assert_eq!(issues[0].first_seen, now());
        assert_eq!(issues[0].last_seen, later());
    }

    #[test]
    fn missing_fingerprint_marks_resolved() {
        let s = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        let issues = merge_issues(Some(&s), vec![], later());
        assert!(issues[0].resolved);
    }

    #[test]
    fn round_trip_through_disk() {
        let dir = std::env::temp_dir().join(format!("dslp-engine-state-v1-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        save(&dir, &s).unwrap();
        let loaded = load(&dir).unwrap().unwrap();
        assert_eq!(loaded.version, STATE_VERSION);
        assert_eq!(loaded.issues.len(), 1);
    }
}
