use std::collections::{HashMap, HashSet};
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use dslp_core::{Finding, Issue, Score};

/// Current on-disk schema version. Bumped to 2 when `Issue.resolved: bool`
/// became `Issue.status: Status`. Older state is discarded on load.
pub const STATE_VERSION: u32 = 2;

/// Resolved issues that stay undetected this long are dropped from state, so
/// it doesn't grow without bound. A later re-detection is then a new issue.
const RESOLVED_RETENTION_DAYS: i64 = 90;

/// On-disk state for a single project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub version: u32,
    pub last_scan: DateTime<Utc>,
    pub score: Score,
    pub issues: Vec<Issue>,
}

/// State lives at `.dslp/state.json` by default, or `.dslp/state-<lang>.json`
/// when scoped to one language plugin — each `--lang` gets its own
/// independent issue list and score, so a monorepo can track e.g. `go` and
/// `typescript` health separately from the same `--path`.
pub fn state_path(project_root: &Path, lang: Option<&str>) -> std::path::PathBuf {
    let filename = match lang {
        Some(id) => format!("state-{id}.json"),
        None => "state.json".to_string(),
    };
    project_root.join(".dslp").join(filename)
}

/// Read state. Returns `Ok(None)` if the file doesn't exist OR if it's an
/// older schema version (we discard incompatible local cache rather than
/// migrate — a scan regenerates everything except wontfix marks).
pub fn load(project_root: &Path, lang: Option<&str>) -> anyhow::Result<Option<State>> {
    let path = state_path(project_root, lang);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)?;
    // Peek at the version before committing to the full deserialize so a v1
    // file doesn't produce a hard error.
    let version = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v.get("version").and_then(|n| n.as_u64()))
        .unwrap_or(0);
    if version != STATE_VERSION as u64 {
        return Ok(None);
    }
    let state: State = serde_json::from_str(&text)?;
    Ok(Some(state))
}

pub fn save(project_root: &Path, lang: Option<&str>, state: &State) -> anyhow::Result<()> {
    let path = state_path(project_root, lang);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Write-then-rename so a crash mid-write can't leave a truncated state file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(state)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Reconcile a fresh scan's findings against the previous issue list,
/// producing the new issue list. Pure — does not score. Truth table:
///
/// | prior      | re-detected | result            |
/// |------------|-------------|-------------------|
/// | (none)     | yes         | Open (new)        |
/// | Open       | yes         | Open              |
/// | Resolved   | yes         | Open (regression) |
/// | Wontfix    | yes         | Wontfix (kept)    |
/// | any        | no          | Resolved          |
///
/// Resolved issues untouched for `RESOLVED_RETENTION_DAYS` are dropped.
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
                existing.refresh(f, now); // preserves Wontfix, else → Open
                by_fp.insert(existing.fingerprint.clone(), existing);
            }
            None => {
                let new = Issue::from_new_finding(f, now);
                by_fp.insert(new.fingerprint.clone(), new);
            }
        }
    }

    // Any prior issue not re-detected this scan is resolved (gone).
    for issue in by_fp.values_mut() {
        if !current_fps.contains(&issue.fingerprint) {
            issue.resolve();
        }
    }

    let cutoff = now - chrono::Duration::days(RESOLVED_RETENTION_DAYS);
    let mut issues: Vec<Issue> = by_fp
        .into_values()
        .filter(|i| !(i.status.is_resolved() && i.last_seen < cutoff))
        .collect();
    issues.sort_by(|a, b| a.fingerprint.cmp(&b.fingerprint));
    issues
}

/// Find exactly one issue by fingerprint prefix. Returns the index, or an
/// error describing no-match / ambiguous-match.
pub fn find_by_prefix(issues: &[Issue], prefix: &str) -> anyhow::Result<usize> {
    let matches: Vec<usize> = issues
        .iter()
        .enumerate()
        .filter(|(_, i)| i.fingerprint.starts_with(prefix))
        .map(|(idx, _)| idx)
        .collect();
    match matches.as_slice() {
        [] => Err(anyhow::anyhow!("no issue matches fingerprint '{}'", prefix)),
        [idx] => Ok(*idx),
        many => {
            let fps: Vec<&str> = many.iter().map(|&i| issues[i].fingerprint.as_str()).collect();
            Err(anyhow::anyhow!(
                "fingerprint '{}' is ambiguous, matches {}: {}",
                prefix,
                many.len(),
                fps.join(", ")
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dslp_core::{Severity, Span, Status};
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
            Some(Span::single_line(1, 0, 0)),
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
    fn new_fingerprint_starts_open() {
        let issues = merge_issues(None, vec![finding("a.rs", "x")], now());
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_open());
        assert_eq!(issues[0].first_seen, now());
    }

    #[test]
    fn open_redetected_stays_open_preserves_first_seen() {
        let f = finding("a.rs", "x");
        let s = state_with(merge_issues(None, vec![f.clone()], now()));
        let issues = merge_issues(Some(&s), vec![f], later());
        assert!(issues[0].is_open());
        assert_eq!(issues[0].first_seen, now());
        assert_eq!(issues[0].last_seen, later());
    }

    #[test]
    fn stale_resolved_issues_are_pruned() {
        let mut s = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        s.issues[0].resolve();
        let much_later = now() + chrono::Duration::days(RESOLVED_RETENTION_DAYS + 1);
        assert!(merge_issues(Some(&s), vec![], later()).len() == 1);
        assert!(merge_issues(Some(&s), vec![], much_later).is_empty());
    }

    #[test]
    fn open_not_redetected_becomes_resolved() {
        let s = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        let issues = merge_issues(Some(&s), vec![], later());
        assert!(issues[0].status.is_resolved());
    }

    #[test]
    fn resolved_redetected_reopens_as_regression() {
        let f = finding("a.rs", "x");
        let mut s = state_with(merge_issues(None, vec![f.clone()], now()));
        s.issues[0].resolve();
        let issues = merge_issues(Some(&s), vec![f], later());
        assert!(issues[0].is_open(), "regression should reopen");
    }

    #[test]
    fn resolved_not_redetected_stays_resolved() {
        let mut s = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        s.issues[0].resolve();
        let issues = merge_issues(Some(&s), vec![], later());
        assert!(issues[0].status.is_resolved());
    }

    #[test]
    fn wontfix_redetected_stays_wontfix() {
        let f = finding("a.rs", "x");
        let mut s = state_with(merge_issues(None, vec![f.clone()], now()));
        s.issues[0].suppress("intentional", now());
        let issues = merge_issues(Some(&s), vec![f], later());
        assert!(matches!(issues[0].status, Status::Wontfix { .. }));
    }

    #[test]
    fn wontfix_not_redetected_becomes_resolved() {
        let mut s = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        s.issues[0].suppress("intentional", now());
        let issues = merge_issues(Some(&s), vec![], later());
        assert!(issues[0].status.is_resolved(), "dismissed thing that's gone → resolved");
    }

    #[test]
    fn find_by_prefix_unique_ok() {
        let issues = merge_issues(None, vec![finding("a.rs", "x")], now());
        let fp = &issues[0].fingerprint;
        let idx = find_by_prefix(&issues, &fp[..6]).unwrap();
        assert_eq!(idx, 0);
    }

    #[test]
    fn find_by_prefix_no_match_errors() {
        let issues = merge_issues(None, vec![finding("a.rs", "x")], now());
        assert!(find_by_prefix(&issues, "zzzzzzzz").is_err());
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = std::env::temp_dir().join(format!("dslp-engine-state-v2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let s = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        save(&dir, None, &s).unwrap();
        let loaded = load(&dir, None).unwrap().unwrap();
        assert_eq!(loaded.version, STATE_VERSION);
        assert_eq!(loaded.issues.len(), 1);
    }

    #[test]
    fn scoped_lang_state_is_independent_of_default() {
        let dir = std::env::temp_dir().join(format!("dslp-engine-state-v2-scoped-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let default_state = state_with(merge_issues(None, vec![finding("a.rs", "x")], now()));
        let go_state = state_with(merge_issues(None, vec![finding("a.go", "y")], now()));
        save(&dir, None, &default_state).unwrap();
        save(&dir, Some("go"), &go_state).unwrap();

        let loaded_default = load(&dir, None).unwrap().unwrap();
        let loaded_go = load(&dir, Some("go")).unwrap().unwrap();
        assert_eq!(loaded_default.issues[0].file, PathBuf::from("a.rs"));
        assert_eq!(loaded_go.issues[0].file, PathBuf::from("a.go"));
        assert!(load(&dir, Some("typescript")).unwrap().is_none());
    }

    #[test]
    fn v1_state_is_discarded_on_load() {
        let dir = std::env::temp_dir().join(format!("dslp-engine-state-v1-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".dslp")).unwrap();
        // A v1-shaped file with `resolved: bool` and old Score shape.
        std::fs::write(
            dir.join(".dslp/state.json"),
            r#"{"version":1,"last_scan":"2026-05-01T20:00:00Z","score":{"composite":90.0,"total_loc":100,"finding_count":1,"by_detector":{}},"issues":[]}"#,
        )
        .unwrap();
        let loaded = load(&dir, None).unwrap();
        assert!(loaded.is_none(), "v1 state should be discarded, not error");
    }
}
