//! Nuxt framework detectors for noAIslop.
//!
//! NET-NEW — desloppify ships no Nuxt support, so this is original work
//! rather than a port. Activates when `package.json` declares `nuxt` as a
//! dependency.
//!
//! v0.5 ships ONE detector — `nuxt.composable-naming`: TypeScript or
//! JavaScript files inside a `composables/` directory MUST export a function
//! whose name starts with `use[A-Z|0-9]`. Nuxt 3's auto-import treats files
//! in `composables/` specially; a file there exporting `fetchUser` instead
//! of `useUser` won't be auto-imported under the conventional name and will
//! confuse callers.

mod detection;
mod detectors;

use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct NuxtPlugin;

impl NuxtPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NuxtPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for NuxtPlugin {
    fn id(&self) -> &'static str {
        "nuxt"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts", "js", "mjs", "cjs"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        if !detection::is_nuxt_project(ctx.project_root) {
            return Vec::new();
        }
        let mut findings = Vec::new();
        for fh in files {
            findings.extend(detectors::composable_naming::run(&fh.path, &fh.source));
        }
        findings
    }
}
