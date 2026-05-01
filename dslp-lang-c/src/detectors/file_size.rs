use std::path::Path;

use dslp_base::FileSizeConfig;
use dslp_core::Finding;
use dslp_lang_common::{file_size, FileSizeText};

const TEXT: FileSizeText = FileSizeText {
    message_tail: "Consider splitting into translation units or extracting cohesive groups \
                   of functions/types into their own files.",
    fix_hint: "Group related types + their operations into one file each, or split a \
               grab-bag file into focused submodules with their own header pair.",
};

pub fn run(
    path: &Path,
    source: &str,
    detector_id: &'static str,
    cfg: &FileSizeConfig,
) -> Vec<Finding> {
    file_size(path, source, detector_id, cfg, &TEXT)
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
        assert!(run(Path::new("a.c"), "int x(void) { return 0; }\n", "c.file-size", &cfg()).is_empty());
    }

    #[test]
    fn fires_above_threshold() {
        let r = run(Path::new("a.c"), &"x\n".repeat(8), "c.file-size", &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "c.file-size");
    }
}
