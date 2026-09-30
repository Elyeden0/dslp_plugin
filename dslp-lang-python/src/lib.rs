//! Python language plugin for noAIslop.
//!
//! Claims `.py` and `.pyi` files. Two detectors mirror the other plugins:
//!
//! - `python.file-size`: line-count threshold
//! - `python.complexity`: cyclomatic over `if_statement`, `elif_clause`,
//!   `for_statement`, `while_statement`, `except_clause`, conditional
//!   expressions (`x if c else y`), match-statement `case_clause`, plus
//!   `and` / `or` boolean operators.
//! - `python.length`: function length

mod detectors;

use dslp_base::Config;
use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct PythonPlugin;

impl PythonPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PythonPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for PythonPlugin {
    fn id(&self) -> &'static str {
        "python"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["py", "pyi"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        let cfg: Config = serde_json::from_value(ctx.config.clone()).unwrap_or_default();
        let mut findings = Vec::new();
        for fh in files {
            let Ok(tree) = dslp_base::parse("python", &fh.source) else {
                continue;
            };
            findings.extend(detectors::file_size::run(
                &fh.path,
                &fh.source,
                &cfg.detectors.file_size_for("python"),
            ));
            findings.extend(detectors::complexity::run(
                &fh.path,
                &fh.source,
                &tree,
                &cfg.detectors.complexity_for("python"),
            ));
            findings.extend(detectors::slop::run(&fh.path, &fh.source, &tree));
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
        let p = PythonPlugin::new();
        assert_eq!(p.id(), "python");
        assert_eq!(p.extensions(), &["py", "pyi"]);
    }

    #[test]
    fn empty_input_yields_no_findings() {
        let p = PythonPlugin::new();
        let v = serde_json::json!({});
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let findings = p.scan(&ctx, &[]);
        assert!(findings.is_empty());
    }

    #[test]
    fn complex_python_function_fires() {
        let p = PythonPlugin::new();
        let v = serde_json::json!({});
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let src = r#"
def tangle(x):
    if x > 0:
        if x < 10:
            return x
        elif x < 20:
            return x * 2
        elif x < 30:
            return x * 3
        elif x < 40:
            return x * 4
        elif x < 50:
            return x * 5
    while x > 100:
        if x % 2 == 0:
            return x
        if x % 3 == 0:
            return x
    for i in range(100):
        if i > 50:
            return i
    return x
"#;
        let findings = p.scan(&ctx, &[fh("a.py", src)]);
        assert!(
            findings.iter().any(|f| f.detector == "python.complexity"),
            "expected python.complexity, got: {:?}",
            findings
        );
    }
}
