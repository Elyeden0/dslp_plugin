use std::path::Path;

use dslp_base::FileSizeConfig;
use dslp_core::Finding;
use dslp_lang_common::{file_size, FileSizeText};

const TEXT: FileSizeText = FileSizeText {
    message_tail: "Consider splitting into a package or extracting cohesive units into their own modules.",
    fix_hint: "Group related classes or functions into one module each, or split a \
               utilities grab-bag into focused submodules.",
};

pub fn run(path: &Path, source: &str, cfg: &FileSizeConfig) -> Vec<Finding> {
    file_size(path, source, "python.file-size", cfg, &TEXT)
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
        assert!(run(Path::new("a.py"), "def x():\n    return 0\n", &cfg()).is_empty());
    }

    #[test]
    fn fires_above_threshold() {
        let r = run(Path::new("a.py"), &"x = 1\n".repeat(8), &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "python.file-size");
    }
}
