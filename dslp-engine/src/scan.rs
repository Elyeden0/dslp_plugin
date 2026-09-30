use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use dslp_base::{discover_files, load_config, Config};
use dslp_core::{FileHandle, Finding, LanguagePlugin};
use rayon::prelude::*;

use crate::delta::{delta, ScanDelta};
use crate::scoring::score_issues;
use crate::state::{load, merge_issues, save, State, STATE_VERSION};

/// Result of a successful scan.
pub struct ScanReport {
    pub state: State,
    pub config: Config,
    pub total_loc: u64,
    pub files_scanned: usize,
    pub delta: ScanDelta,
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

    // Walk the tree once for every extension any plugin claims, and read each
    // file once — several plugins (ts, react, nextjs, nuxt) share extensions.
    let all_exts: Vec<&str> = plugins
        .iter()
        .flat_map(|p| p.extensions().iter().copied())
        .collect();
    let mut sources: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut handles_per_plugin: Vec<Vec<FileHandle>> = Vec::new();
    let discovered = discover_files(project_root, &all_exts, &config.scan.exclude);
    let mut total_loc: u64 = 0;
    let mut files_scanned = 0usize;
    for plugin in plugins {
        let mut handles = Vec::new();
        for abs in discovered.iter().filter(|p| claims(plugin.extensions(), p)) {
            let source = sources.entry(abs.clone()).or_insert_with(|| {
                let text = std::fs::read_to_string(abs).ok();
                if let Some(t) = &text {
                    total_loc += t.lines().count() as u64;
                    files_scanned += 1;
                }
                text
            });
            if let Some(source) = source {
                // Plugins see (and findings record) paths relative to the
                // project root, so `scan .` and `scan /abs/path` agree.
                let rel = abs.strip_prefix(project_root).unwrap_or(abs).to_path_buf();
                handles.push(FileHandle {
                    path: rel,
                    source: source.clone(),
                });
            }
        }
        handles_per_plugin.push(handles);
    }

    let cfg_ref = &cfg_value;
    let per_plugin: Vec<Vec<Finding>> = plugins
        .par_iter()
        .zip(handles_per_plugin.par_iter())
        .map(|(plugin, handles)| {
            let ctx = dslp_core::ScanContext {
                project_root,
                config: cfg_ref,
            };
            plugin.scan(&ctx, handles)
        })
        .collect();
    let all_findings = disambiguate(crate::overrides::apply(
        per_plugin.into_iter().flatten().collect(),
        &config.detectors,
    ));

    let now = Utc::now();
    let prev = load(project_root, lang)?;
    let sources: HashMap<PathBuf, &str> = handles_per_plugin
        .iter()
        .flatten()
        .map(|h| (h.path.clone(), h.source.as_str()))
        .collect();
    let ignored = crate::inline::ignored(&all_findings, &sources);
    let mut issues = merge_issues(prev.as_ref(), all_findings, now);
    for issue in issues.iter_mut().filter(|i| i.is_open()) {
        if let Some(reason) = ignored.get(&issue.fingerprint) {
            issue.suppress(reason.clone(), now);
        }
    }
    let delta = delta(prev.as_ref(), &issues);
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
        delta,
    })
}

fn claims(extensions: &[&str], path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

/// Give findings that share a fingerprint (e.g. two same-named functions in
/// one file) distinct, order-stable fingerprints.
fn disambiguate(findings: Vec<Finding>) -> Vec<Finding> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    findings
        .into_iter()
        .map(|f| {
            let n = seen.entry(f.fingerprint.clone()).or_insert(0);
            let out = f.with_occurrence(*n);
            *n += 1;
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dslp_core::Severity;

    #[test]
    fn disambiguate_makes_duplicate_fingerprints_unique() {
        let mk = || {
            Finding::new(
                "d",
                Severity::Low,
                PathBuf::from("a.rs"),
                None,
                "m",
                None,
                "same",
            )
        };
        let out = disambiguate(vec![mk(), mk(), mk()]);
        let mut fps: Vec<_> = out.iter().map(|f| f.fingerprint.clone()).collect();
        fps.sort();
        fps.dedup();
        assert_eq!(fps.len(), 3);
        assert_eq!(out[0].fingerprint, mk().fingerprint);
    }
}
