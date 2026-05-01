//! File-size detector for TypeScript / JavaScript. Thin wrapper over the
//! shared primitive — only the detector id and wording are TS-specific.

use std::path::Path;

use dslp_base::FileSizeConfig;
use dslp_core::Finding;
use dslp_lang_common::{file_size, FileSizeText};

const TEXT: FileSizeText = FileSizeText {
    message_tail: "Consider splitting along module or component boundaries.",
    fix_hint: "Identify a coherent sub-unit (one component + its hooks, or a related \
               group of utilities) and extract it into a new module.",
};

pub fn run(path: &Path, source: &str, cfg: &FileSizeConfig) -> Vec<Finding> {
    file_size(path, source, "ts.file-size", cfg, &TEXT)
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
        assert!(run(Path::new("a.ts"), "let a = 1;\nlet b = 2;\n", &cfg()).is_empty());
    }

    #[test]
    fn fires_above_threshold() {
        let r = run(Path::new("a.ts"), &"x\n".repeat(7), &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "ts.file-size");
    }
}
