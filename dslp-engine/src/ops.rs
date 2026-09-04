//! Mutating operations on persisted state that do NOT rescan: `resolve` and
//! `suppress`. Each loads state, mutates one issue found by fingerprint
//! prefix, re-scores from the mutated issue list, persists, and returns the
//! affected issue plus the fresh score.

use std::path::Path;

use chrono::Utc;
use dslp_core::{Issue, Score};

use crate::scoring::score_issues;
use crate::state::{find_by_prefix, load, save};

pub struct OpResult {
    pub issue: Issue,
    pub score: Score,
}

fn load_or_err(project_root: &Path, lang: Option<&str>) -> anyhow::Result<crate::state::State> {
    load(project_root, lang)?.ok_or_else(|| {
        anyhow::anyhow!(
            "no scan found at {} — run `dslp scan` first",
            crate::state::state_path(project_root, lang).display()
        )
    })
}

/// Mark the issue matching `fp_prefix` as Resolved (optimistic — a later scan
/// that re-detects it will reopen it as a regression).
pub fn resolve(project_root: &Path, lang: Option<&str>, fp_prefix: &str) -> anyhow::Result<OpResult> {
    let mut state = load_or_err(project_root, lang)?;
    let idx = find_by_prefix(&state.issues, fp_prefix)?;
    state.issues[idx].resolve();
    finish(project_root, lang, state, idx)
}

/// Mark the issue matching `fp_prefix` as Wontfix with a reason (a standing
/// decision that survives rescans but still counts toward the strict score).
pub fn suppress(project_root: &Path, lang: Option<&str>, fp_prefix: &str, reason: &str) -> anyhow::Result<OpResult> {
    if reason.trim().is_empty() {
        return Err(anyhow::anyhow!("suppress requires a non-empty --reason"));
    }
    let mut state = load_or_err(project_root, lang)?;
    let idx = find_by_prefix(&state.issues, fp_prefix)?;
    state.issues[idx].suppress(reason, Utc::now());
    finish(project_root, lang, state, idx)
}

fn finish(project_root: &Path, lang: Option<&str>, mut state: crate::state::State, idx: usize) -> anyhow::Result<OpResult> {
    state.score = score_issues(&state.issues, state.score.total_loc);
    let issue = state.issues[idx].clone();
    save(project_root, lang, &state)?;
    Ok(OpResult {
        issue,
        score: state.score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{merge_issues, save, State, STATE_VERSION};
    use chrono::DateTime;
    use dslp_core::{Finding, Severity, Span, Status};
    use std::path::PathBuf;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-01T20:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn fixture_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dslp-ops-{}-{}", label, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn seed(dir: &Path) -> String {
        let f = Finding::new(
            "d",
            Severity::High,
            PathBuf::from("a.rs"),
            Some(Span::single_line(1, 0, 0)),
            "msg",
            None,
            "snip",
        );
        let issues = merge_issues(None, vec![f], now());
        let fp = issues[0].fingerprint.clone();
        let state = State {
            version: STATE_VERSION,
            last_scan: now(),
            score: crate::scoring::score_issues(&issues, 1000),
            issues,
        };
        save(dir, None, &state).unwrap();
        fp
    }

    #[test]
    fn resolve_marks_resolved_and_rescores() {
        let dir = fixture_dir("resolve");
        let fp = seed(&dir);
        let res = resolve(&dir, None, &fp[..6]).unwrap();
        assert!(res.issue.status.is_resolved());
        assert!((res.score.lenient - 100.0).abs() < 1e-9, "resolving the only issue → lenient 100");
    }

    #[test]
    fn suppress_requires_reason() {
        let dir = fixture_dir("noreason");
        let fp = seed(&dir);
        assert!(suppress(&dir, None, &fp[..6], "   ").is_err());
    }

    #[test]
    fn suppress_sets_wontfix_and_widens_gap() {
        let dir = fixture_dir("suppress");
        let fp = seed(&dir);
        let res = suppress(&dir, None, &fp[..6], "acceptable in this context").unwrap();
        assert!(matches!(res.issue.status, Status::Wontfix { .. }));
        // lenient improves (no open issues), strict stays penalized → gap > 0
        assert!((res.score.lenient - 100.0).abs() < 1e-9);
        assert!(res.score.strict < 100.0);
        assert!(res.score.gap() > 0.0);
    }

    #[test]
    fn resolve_without_scan_errors() {
        let dir = fixture_dir("noscan");
        assert!(resolve(&dir, None, "abc").is_err());
    }
}
