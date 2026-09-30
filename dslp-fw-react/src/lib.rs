//! React framework detectors for noAIslop.
//!
//! Implemented as a `LanguagePlugin` that claims `.ts` / `.tsx` / `.js` /
//! `.jsx`. The plugin self-gates on a project-level detection step (looking
//! for `react` as a dependency in package.json, or `.tsx` files in the tree)
//! and skips the scan entirely if React isn't in use.
//!
//! v0.3 ships ONE detector — `react.hook-return-bloat`, ported from
//! desloppify: a custom hook (function whose name starts with `use[A-Z]`)
//! that returns an object literal with > 12 fields. The threshold is the
//! same as the reference implementation. The signal is "this hook is
//! probably doing too many things" — a real refactor candidate.

mod detection;
mod detectors;

use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct ReactPlugin;

impl ReactPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ReactPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for ReactPlugin {
    fn id(&self) -> &'static str {
        "react"
    }

    fn extensions(&self) -> &'static [&'static str] {
        // Same set as the TS plugin — we run on the same files but emit
        // additional, framework-specific findings.
        &["ts", "tsx", "js", "jsx", "cjs", "mjs"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        if !detection::is_react_project(ctx.project_root, files) {
            return Vec::new();
        }
        let mut findings = Vec::new();
        for fh in files {
            let lang_id = match fh.path.extension().and_then(|s| s.to_str()) {
                Some("tsx") => "tsx",
                Some("ts") => "typescript",
                Some("jsx") | Some("js") | Some("cjs") | Some("mjs") => "javascript",
                _ => continue,
            };
            let Ok(tree) = dslp_base::parse(lang_id, &fh.source) else {
                continue;
            };
            findings.extend(detectors::hook_return_bloat::run(
                &fh.path, &fh.source, &tree,
            ));
        }
        findings
    }
}
