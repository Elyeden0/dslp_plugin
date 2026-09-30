//! TypeScript / JavaScript language plugin for noAIslop (v0.2 in progress).
//!
//! Claims `.ts`, `.tsx`, `.js`, `.jsx`, `.cjs`, `.mjs` files. Two detectors
//! ported from the Rust plugin: file size and per-function complexity.
//! Dead-export detection is intentionally NOT in this slice — the TS module
//! system (`export` / `export default` / re-exports / dynamic imports / JSX
//! references) needs its own design pass.

mod detection;
mod detectors;

use dslp_base::Config;
use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct TsPlugin;

impl TsPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TsPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for TsPlugin {
    fn id(&self) -> &'static str {
        "typescript"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx", "js", "jsx", "cjs", "mjs"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        let cfg: Config = serde_json::from_value(ctx.config.clone()).unwrap_or_default();
        let parsed: Vec<(&FileHandle, tree_sitter::Tree)> = files
            .iter()
            .filter_map(|fh| {
                let lang_id = lang_for_extension(&fh.path)?;
                let tree = dslp_base::parse(lang_id, &fh.source).ok()?;
                Some((fh, tree))
            })
            .collect();

        let mut findings = Vec::new();
        for (fh, tree) in &parsed {
            findings.extend(dslp_lang_common::comments::run(
                &fh.path, &fh.source, "ts", "//",
            ));
            findings.extend(detectors::file_size::run(
                &fh.path,
                &fh.source,
                &cfg.detectors.file_size_for("typescript"),
            ));
            findings.extend(detectors::complexity::run(
                &fh.path,
                &fh.source,
                tree,
                &cfg.detectors.complexity_for("typescript"),
            ));
            findings.extend(detectors::slop::run(
                &fh.path,
                &fh.source,
                tree,
                &cfg.detectors.slop,
            ));
        }
        findings.extend(dead_exports(ctx, &cfg, &parsed));
        findings
    }
}

/// Exported names nobody else mentions. Skipped for library packages, whose
/// exports are the public API.
fn dead_exports(
    ctx: &ScanContext,
    cfg: &Config,
    parsed: &[(&FileHandle, tree_sitter::Tree)],
) -> Vec<Finding> {
    if detection::is_library(ctx.project_root) {
        return Vec::new();
    }
    let analyzed: Vec<_> = parsed
        .iter()
        .map(|(fh, tree)| (fh, detectors::dead_export::analyze(&fh.source, tree)))
        .collect();
    let refs: std::collections::HashSet<&String> = analyzed
        .iter()
        .flat_map(|(_, (_, refs))| refs.iter())
        .collect();
    let mut out = Vec::new();
    for (fh, (decls, _)) in &analyzed {
        let path = fh.path.to_string_lossy();
        let skipped = detectors::dead_export::is_entry_or_convention(&fh.path)
            || cfg
                .detectors
                .dead_pub
                .ignore_paths
                .iter()
                .any(|p| path.contains(p.as_str()));
        if skipped {
            continue;
        }
        out.extend(
            decls
                .iter()
                .filter(|d| !refs.contains(&d.name))
                .map(|d| detectors::dead_export::finding(&fh.path, d)),
        );
    }
    out
}

fn lang_for_extension(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?;
    match ext.to_ascii_lowercase().as_str() {
        "ts" => Some("typescript"),
        "tsx" => Some("tsx"),
        "js" | "cjs" | "mjs" | "jsx" => Some("javascript"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ctx_value() -> serde_json::Value {
        serde_json::json!({})
    }

    fn fh(path: &str, source: &str) -> FileHandle {
        FileHandle {
            path: PathBuf::from(path),
            source: source.into(),
        }
    }

    #[test]
    fn id_and_extensions() {
        let p = TsPlugin::new();
        assert_eq!(p.id(), "typescript");
        assert!(p.extensions().contains(&"ts"));
        assert!(p.extensions().contains(&"tsx"));
        assert!(p.extensions().contains(&"js"));
    }

    #[test]
    fn empty_input_yields_no_findings() {
        let p = TsPlugin::new();
        let v = ctx_value();
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let findings = p.scan(&ctx, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn typescript_file_complexity_detected() {
        let p = TsPlugin::new();
        let v = ctx_value();
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let src = r#"
function tangle(x: number): number {
    if (x > 0) {
        if (x < 10) return x;
        if (x < 20) return x * 2;
        if (x < 30) return x * 3;
        if (x < 40) return x * 4;
    }
    while (x > 100) {
        if (x % 2 === 0) return x;
        if (x % 3 === 0) return x;
    }
    for (let i = 0; i < 100; i++) {
        if (i > 50) return i;
    }
    return x;
}
"#;
        let findings = p.scan(&ctx, &[fh("a.ts", src)]);
        assert!(
            findings.iter().any(|f| f.detector == "ts.complexity"),
            "expected ts.complexity, got: {:?}",
            findings
        );
    }

    #[test]
    fn jsx_file_parses_via_tsx_grammar() {
        let p = TsPlugin::new();
        let v = ctx_value();
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let src = "export const Hello = () => <div>hi</div>;\n";
        let findings = p.scan(&ctx, &[fh("a.tsx", src)]);
        // Just assert the scan didn't panic and returned an iterable.
        let _ = findings.len();
    }
}
