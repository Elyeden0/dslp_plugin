use std::path::Path;

use dslp_base::FileSizeConfig;
use dslp_core::Finding;
use dslp_lang_common::{file_size, FileSizeText};

const TEXT: FileSizeText = FileSizeText {
    message_tail: "Consider splitting along module boundaries.",
    fix_hint: "Identify a coherent sub-unit (one struct + impls, or a related group of \
               functions) and extract it into a new module.",
};

/// Flags `.rs` files over the line threshold. Thin wrapper over the shared
/// file-size primitive — only the detector id and wording are Rust-specific.
pub fn run(path: &Path, source: &str, cfg: &FileSizeConfig) -> Vec<Finding> {
    file_size(path, source, "rust.file-size", cfg, &TEXT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dslp_core::Severity;

    fn cfg() -> FileSizeConfig {
        FileSizeConfig {
            threshold: 5,
            severity_at: 10,
        }
    }

    #[test]
    fn does_not_fire_below_threshold() {
        assert!(run(Path::new("a.rs"), "fn a() {}\nfn b() {}\n", &cfg()).is_empty());
    }

    #[test]
    fn fires_at_threshold_with_medium() {
        let r = run(Path::new("a.rs"), "1\n2\n3\n4\n5\n", &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].severity, Severity::Medium);
        assert_eq!(r[0].detector, "rust.file-size");
    }

    #[test]
    fn escalates_to_high_above_severity_at() {
        let r = run(Path::new("a.rs"), &"x\n".repeat(12), &cfg());
        assert_eq!(r[0].severity, Severity::High);
    }
}
