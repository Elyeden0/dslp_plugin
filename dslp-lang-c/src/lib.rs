//! C language plugin for noAIslop. Claims `.c` and `.h` files.
//!
//! Caveat for `.h`: tree-sitter-c parses headers as a translation unit. This
//! works for declaration-heavy headers but the parse may produce ERROR nodes
//! for headers that rely on macro context (e.g. macros that expand into
//! statements). We accept that — file-size still works, and complexity will
//! simply skip functions that don't parse cleanly.

mod detectors;

use dslp_base::Config;
use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct CPlugin;

impl CPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for CPlugin {
    fn id(&self) -> &'static str {
        "c"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["c", "h"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        let cfg: Config = serde_json::from_value(ctx.config.clone()).unwrap_or_default();
        let mut findings = Vec::new();
        for fh in files {
            let Ok(tree) = dslp_base::parse("c", &fh.source) else {
                continue;
            };
            findings.extend(detectors::file_size::run(
                &fh.path,
                &fh.source,
                "c.file-size",
                &cfg.detectors.file_size_for("c"),
            ));
            findings.extend(detectors::complexity::run(
                &fh.path,
                &fh.source,
                &tree,
                &cfg.detectors.complexity_for("c"),
            ));
        }
        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fh(path: &str, source: &str) -> FileHandle {
        FileHandle {
            path: PathBuf::from(path),
            source: source.into(),
        }
    }

    #[test]
    fn id_and_extensions() {
        let p = CPlugin::new();
        assert_eq!(p.id(), "c");
        assert_eq!(p.extensions(), &["c", "h"]);
    }

    #[test]
    fn empty_input_yields_no_findings() {
        let p = CPlugin::new();
        let v = serde_json::json!({});
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let findings = p.scan(&ctx, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn complex_c_function_fires() {
        let p = CPlugin::new();
        let v = serde_json::json!({});
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let src = r#"
int tangle(int x) {
    if (x > 0) {
        if (x < 10) return x;
        if (x < 20) return x * 2;
        if (x < 30) return x * 3;
        if (x < 40) return x * 4;
        if (x < 50) return x * 5;
    }
    while (x > 100) {
        if (x % 2 == 0) return x;
        if (x % 3 == 0) return x;
        if (x % 5 == 0) return x;
    }
    for (int i = 0; i < 100; i++) {
        if (i > 50) return i;
    }
    return x;
}
"#;
        let findings = p.scan(&ctx, &[fh("a.c", src)]);
        assert!(
            findings.iter().any(|f| f.detector == "c.complexity"),
            "expected c.complexity, got: {:?}",
            findings
        );
    }
}
