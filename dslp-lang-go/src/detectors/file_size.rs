use std::path::Path;

use dslp_base::FileSizeConfig;
use dslp_core::Finding;
use dslp_lang_common::{file_size, FileSizeText};

const TEXT: FileSizeText = FileSizeText {
    message_tail: "Consider splitting into multiple files along package or interface boundaries.",
    fix_hint: "Group related types + their methods into one file each, or split a \
               grab-bag file into focused submodules within the same package.",
};

pub fn run(path: &Path, source: &str, cfg: &FileSizeConfig) -> Vec<Finding> {
    file_size(path, source, "go.file-size", cfg, &TEXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> FileSizeConfig {
        FileSizeConfig {
            threshold: 5,
            severity_at: 10,
        }
    }

    #[test]
    fn does_not_fire_below_threshold() {
        assert!(run(Path::new("a.go"), "package main\nfunc x() {}\n", &cfg()).is_empty());
    }

    #[test]
    fn fires_above_threshold() {
        let r = run(Path::new("a.go"), &"x\n".repeat(8), &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "go.file-size");
    }
}
