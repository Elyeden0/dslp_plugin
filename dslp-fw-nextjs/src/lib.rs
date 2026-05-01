//! Next.js framework detectors for noAIslop.
//!
//! Implemented as a self-gating `LanguagePlugin` (same shape as the React
//! plugin). Detection is strictly "package.json includes `next` as a
//! dependency" — Next.js projects always have a package.json, and a `.tsx`
//! file alone isn't enough signal.
//!
//! v0.4 ships ONE detector — `nextjs.use-client-not-first`, ported from
//! desloppify. Next.js's `'use client'` directive is only valid as the very
//! first meaningful line of a module (after comments and a possible
//! shebang). Misplacing it makes Next silently treat the file as a server
//! component, which is a hard-to-spot bug in app-router projects.

mod detection;
mod detectors;

use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct NextjsPlugin;

impl NextjsPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NextjsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for NextjsPlugin {
    fn id(&self) -> &'static str {
        "nextjs"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx", "js", "jsx", "cjs", "mjs"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        if !detection::is_nextjs_project(ctx.project_root) {
            return Vec::new();
        }
        let mut findings = Vec::new();
        for fh in files {
            findings.extend(detectors::use_client_not_first::run(&fh.path, &fh.source));
        }
        findings
    }
}
