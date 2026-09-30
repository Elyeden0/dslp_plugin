use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use dslp_base::{discover_files, load_config, Config};
use dslp_core::{FileHandle, Finding, Issue, LanguagePlugin};
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
///
/// `only` restricts *reporting* to those project-relative files: the whole
/// project is still analysed (cross-file detectors need the full corpus), but
/// findings elsewhere are dropped and the previous issues for files outside
/// `only` are carried over untouched instead of being marked resolved.
pub fn run_scan(
    project_root: &Path,
    plugins: &[Box<dyn LanguagePlugin>],
    lang: Option<&str>,
    only: Option<&HashSet<PathBuf>>,
) -> anyhow::Result<ScanReport> {
    let config = load_config(project_root)?;
    let cfg_value = serde_json::to_value(&config)?;

    // Walk the tree once for every extension any plugin claims, and read each
    // file once — several plugins (ts, react, nextjs, nuxt) share extensions.
    let all_exts: Vec<&str> = plugins
        .iter()
        .flat_map(|p| p.extensions().iter().copied())
        .collect();
    let mut sources: HashMap<PathBuf, Option<Arc<str>>> = HashMap::new();
    let mut handles_per_plugin: Vec<Vec<FileHandle>> = Vec::new();
    let discovered = discover_files(project_root, &all_exts, &config.scan.exclude);
    let mut total_loc: u64 = 0;
    let mut files_scanned = 0usize;
    for plugin in plugins {
        let mut handles = Vec::new();
        for abs in discovered.iter().filter(|p| claims(plugin.extensions(), p)) {
            let source = sources.entry(abs.clone()).or_insert_with(|| {
                let text: Option<Arc<str>> = std::fs::read_to_string(abs).ok().map(Arc::from);
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
    let mut all_findings = disambiguate(crate::overrides::apply(
        per_plugin.into_iter().flatten().collect(),
        &config.detectors,
    ));
    if let Some(only) = only {
        all_findings.retain(|f| only.contains(&f.file));
    }

    let now = Utc::now();
    let prev = load(project_root, lang)?;
    let sources: HashMap<PathBuf, &str> = handles_per_plugin
        .iter()
        .flatten()
        .map(|h| (h.path.clone(), &*h.source))
        .collect();
    let ignored = crate::inline::ignored(&all_findings, &sources);
    let mut issues = merge_issues(prev.as_ref(), all_findings, now);
    for issue in issues.iter_mut().filter(|i| i.is_open()) {
        if let Some(reason) = ignored.get(&issue.fingerprint) {
            issue.suppress(reason.clone(), now);
        }
    }
    if let (Some(only), Some(prev)) = (only, prev.as_ref()) {
        restore_out_of_scope(&mut issues, prev, only);
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

/// Put back the previous version of every issue outside `only`: the merge
/// treats anything not re-detected as resolved, which is wrong for files a
/// partial scan never reported on.
fn restore_out_of_scope(issues: &mut [Issue], prev: &State, only: &HashSet<PathBuf>) {
    let before: HashMap<&str, &Issue> = prev
        .issues
        .iter()
        .map(|i| (i.fingerprint.as_str(), i))
        .collect();
    for issue in issues.iter_mut().filter(|i| !only.contains(&i.file)) {
        if let Some(old) = before.get(issue.fingerprint.as_str()) {
            *issue = (*old).clone();
        }
    }
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
    fn restore_out_of_scope_keeps_previous_status_outside_the_scope() {
        use crate::state::merge_issues;
        let mk = |file: &str| {
            Finding::new(
                "d",
                Severity::Low,
                PathBuf::from(file),
                None,
                "m",
                None,
                "s",
            )
        };
        let now = chrono::Utc::now();
        let prev = State {
            version: STATE_VERSION,
            last_scan: now,
            score: dslp_core::Score::perfect(),
            issues: merge_issues(None, vec![mk("in.rs"), mk("out.rs")], now),
        };
        // A partial scan that reported nothing: the merge resolves both.
        let mut issues = merge_issues(Some(&prev), vec![], now);
        assert!(issues.iter().all(|i| i.status.is_resolved()));
        let only = HashSet::from([PathBuf::from("in.rs")]);
        restore_out_of_scope(&mut issues, &prev, &only);
        let status = |file: &str| {
            issues
                .iter()
                .find(|i| i.file == Path::new(file))
                .unwrap()
                .status
                .clone()
        };
        assert!(status("in.rs").is_resolved(), "in scope: really gone");
        assert!(status("out.rs").is_open(), "out of scope: untouched");
    }

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
