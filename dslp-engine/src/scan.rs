use std::path::Path;

use chrono::Utc;
use dslp_base::{discover_files, load_config, Config};
use dslp_core::{FileHandle, LanguagePlugin};

use crate::scoring::score_issues;
use crate::state::{load, merge_issues, save, State, STATE_VERSION};

/// Result of a successful scan.
pub struct ScanReport {
    pub state: State,
    pub config: Config,
    pub total_loc: u64,
    pub files_scanned: usize,
}

/// End-to-end scan: discover files, dispatch to plugins, reconcile against
/// prior state (preserving wontfix), score the reconciled issues, persist.
///
/// `lang` scopes the scan to a single plugin's state file
/// (`.dslp/state-<lang>.json`) instead of the shared `.dslp/state.json` —
/// `plugins` must already be filtered to just that plugin by the caller.
pub fn run_scan(
    project_root: &Path,
    plugins: &[Box<dyn LanguagePlugin>],
    lang: Option<&str>,
) -> anyhow::Result<ScanReport> {
    let config = load_config(project_root)?;
    let cfg_value = serde_json::to_value(&config)?;

    let mut all_findings = Vec::new();
    let mut total_loc: u64 = 0;
    let mut files_scanned = 0usize;
    for plugin in plugins {
        let files = discover_files(project_root, plugin.extensions(), &config.scan.exclude);
        let handles: Vec<FileHandle> = files
            .into_iter()
            .filter_map(|p| {
                let source = std::fs::read_to_string(&p).ok()?;
                total_loc += source.lines().count() as u64;
                Some(FileHandle { path: p, source })
            })
            .collect();
        files_scanned += handles.len();
        let ctx = dslp_core::ScanContext {
            project_root,
            config: &cfg_value,
        };
        all_findings.extend(plugin.scan(&ctx, &handles));
    }

    let now = Utc::now();
    let prev = load(project_root, lang)?;
    let issues = merge_issues(prev.as_ref(), all_findings, now);
    let score = score_issues(&issues, total_loc);
    let state = State {
        version: STATE_VERSION,
        last_scan: now,
        score,
        issues,
    };
    save(project_root, lang, &state)?;
    Ok(ScanReport {
        state,
        config,
        total_loc,
        files_scanned,
    })
}
