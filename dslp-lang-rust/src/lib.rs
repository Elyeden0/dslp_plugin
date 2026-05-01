//! Rust language plugin for noAIslop.
//!
//! Implements [`dslp_core::LanguagePlugin`] over `.rs` files using
//! tree-sitter-rust. Three detectors: file size, function complexity,
//! and dead public items.

mod detectors;

use std::collections::HashSet;

use dslp_base::Config;
use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct RustPlugin;

impl RustPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for RustPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for RustPlugin {
    fn id(&self) -> &'static str {
        "rust"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        // Resolve config from ctx.config (a serde_json::Value the engine fills
        // from the user's TOML). On any decoding error, fall back to defaults.
        let cfg: Config = serde_json::from_value(ctx.config.clone()).unwrap_or_default();

        // Pass 1: parse each file and collect identifier references corpus-wide.
        // We hold trees in a Vec so pass 2 can reuse them without re-parsing.
        let parsed: Vec<Parsed> = files
            .iter()
            .filter_map(|fh| {
                dslp_base::parse("rust", &fh.source)
                    .ok()
                    .map(|tree| Parsed {
                        path: fh.path.clone(),
                        source: fh.source.clone(),
                        tree,
                    })
            })
            .collect();

        let mut refs: HashSet<String> = HashSet::new();
        for p in &parsed {
            refs.extend(detectors::dead_pub::collect_references(&p.source, &p.tree));
        }

        // Pass 2: run all three detectors per file.
        let mut findings = Vec::new();
        for p in &parsed {
            findings.extend(detectors::file_size::run(
                &p.path,
                &p.source,
                &cfg.detectors.file_size,
            ));
            findings.extend(detectors::complexity::run(
                &p.path,
                &p.source,
                &p.tree,
                &cfg.detectors.complexity,
            ));
            findings.extend(detectors::dead_pub::run(
                &p.path,
                &p.source,
                &p.tree,
                &refs,
                &cfg.detectors.dead_pub,
            ));
        }
        findings
    }
}

struct Parsed {
    path: std::path::PathBuf,
    source: String,
    tree: tree_sitter::Tree,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx() -> serde_json::Value {
        serde_json::json!({})
    }

    fn fh(path: &str, source: &str) -> FileHandle {
        FileHandle {
            path: PathBuf::from(path),
            source: source.to_string(),
        }
    }

    #[test]
    fn id_and_extensions_are_stable() {
        let p = RustPlugin::new();
        assert_eq!(p.id(), "rust");
        assert_eq!(p.extensions(), &["rs"]);
    }

    #[test]
    fn empty_input_yields_no_findings() {
        let p = RustPlugin::new();
        let cfg_val = ctx();
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &cfg_val,
        };
        let findings = p.scan(&ctx, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn detects_unused_pub_across_two_files() {
        let p = RustPlugin::new();
        let cfg_val = ctx();
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &cfg_val,
        };
        let files = vec![
            fh("src/a.rs", "pub fn never_called() {}\n"),
            fh("src/b.rs", "fn somewhere_else() {}\n"),
        ];
        let findings = p.scan(&ctx, &files);
        assert!(
            findings.iter().any(|f| f.detector == "rust.dead-pub-fn"),
            "expected dead-pub finding, got: {:?}",
            findings
        );
    }

    #[test]
    fn pub_referenced_in_other_file_does_not_fire() {
        let p = RustPlugin::new();
        let cfg_val = ctx();
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &cfg_val,
        };
        let files = vec![
            fh("src/a.rs", "pub fn used_externally() {}\n"),
            fh("src/b.rs", "fn caller() { used_externally(); }\n"),
        ];
        let findings = p.scan(&ctx, &files);
        assert!(
            !findings.iter().any(|f| f.detector == "rust.dead-pub-fn"),
            "should not fire for cross-file usage, got: {:?}",
            findings
        );
    }
}
