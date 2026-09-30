//! C++ language plugin for noAIslop. Claims `.cpp`, `.cc`, `.cxx`, `.c++`,
//! `.hpp`, `.hh`, `.hxx`, `.h++` files.
//!
//! Note on header files: many C++ projects use `.h` for both C and C++
//! headers. To avoid double-coverage with the C plugin, we don't claim `.h`.
//! Headers that need C++-aware analysis should use `.hpp` (the dominant
//! convention in modern C++).

mod detectors;

use dslp_base::Config;
use dslp_core::{FileHandle, Finding, LanguagePlugin, ScanContext};

pub struct CppPlugin;

impl CppPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CppPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for CppPlugin {
    fn id(&self) -> &'static str {
        "cpp"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["cpp", "cc", "cxx", "c++", "hpp", "hh", "hxx", "h++"]
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        let cfg: Config = serde_json::from_value(ctx.config.clone()).unwrap_or_default();
        let mut findings = Vec::new();
        for fh in files {
            let Ok(tree) = dslp_base::parse("cpp", &fh.source) else {
                continue;
            };
            findings.extend(dslp_lang_common::comments::run(
                &fh.path, &fh.source, "cpp", "//",
            ));
            findings.extend(detectors::file_size::run(
                &fh.path,
                &fh.source,
                "cpp.file-size",
                &cfg.detectors.file_size_for("cpp"),
            ));
            findings.extend(detectors::complexity::run(
                &fh.path,
                &fh.source,
                &tree,
                &cfg.detectors.complexity_for("cpp"),
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
        let p = CppPlugin::new();
        assert_eq!(p.id(), "cpp");
        assert!(p.extensions().contains(&"cpp"));
        assert!(p.extensions().contains(&"hpp"));
        assert!(!p.extensions().contains(&"h"));
    }

    #[test]
    fn complex_cpp_function_fires() {
        let p = CppPlugin::new();
        let v = serde_json::json!({});
        let ctx = ScanContext {
            project_root: std::path::Path::new("."),
            config: &v,
        };
        let src = r#"
#include <vector>

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
        let findings = p.scan(&ctx, &[fh("a.cpp", src)]);
        assert!(
            findings.iter().any(|f| f.detector == "cpp.complexity"),
            "expected cpp.complexity, got: {:?}",
            findings
        );
    }
}
