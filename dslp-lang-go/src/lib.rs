//! Go language plugin for noAIslop.
//!
//! Claims `.go` files. Three detectors mirror the Rust and TS plugins
//! with Go-specific node-kind sets:
//!
//! - `go.file-size`: line-count threshold
//! - `go.complexity`: cyclomatic over `if_statement`, `for_statement`,
//!   `expression_case` / `default_case` / `communication_case` /
//!   `type_case`, plus `&&` / `||`
//! - `go.length`: function length

mod detectors;

use dslp_base::Config;
use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct GoPlugin;

impl GoPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GoPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for GoPlugin {
    fn id(&self) -> &'static str {
        "go"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        let cfg: Config = serde_json::from_value(ctx.config.clone()).unwrap_or_default();
        let mut findings = Vec::new();
        for fh in files {
            let Ok(tree) = dslp_base::parse("go", &fh.source) else {
                continue;
            };
            findings.extend(dslp_lang_common::comments::run(
                &fh.path, &fh.source, "go", "//",
            ));
            findings.extend(detectors::file_size::run(
                &fh.path,
                &fh.source,
                &cfg.detectors.file_size_for("go"),
            ));
            findings.extend(detectors::complexity::run(
                &fh.path,
                &fh.source,
                &tree,
                &cfg.detectors.complexity_for("go"),
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
        let p = GoPlugin::new();
        assert_eq!(p.id(), "go");
        assert_eq!(p.extensions(), &["go"]);
    }

    #[test]
    fn empty_input_yields_no_findings() {
        let p = GoPlugin::new();
        let v = serde_json::json!({});
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let findings = p.scan(&ctx, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn complex_go_function_fires() {
        let p = GoPlugin::new();
        let v = serde_json::json!({});
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let src = r#"package main

func tangle(x int) int {
    if x > 0 {
        if x < 10 { return x }
        if x < 20 { return x * 2 }
        if x < 30 { return x * 3 }
        if x < 40 { return x * 4 }
        if x < 50 { return x * 5 }
    }
    for i := 0; i < 100; i++ {
        if i > 50 { return i }
        if i > 60 { return i + 1 }
        if i > 70 { return i + 2 }
        if i > 80 { return i + 3 }
    }
    return x
}
"#;
        let findings = p.scan(&ctx, &[fh("a.go", src)]);
        assert!(
            findings.iter().any(|f| f.detector == "go.complexity"),
            "expected go.complexity, got: {:?}",
            findings
        );
    }
}
