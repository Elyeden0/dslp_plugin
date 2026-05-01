use std::path::Path;

use dslp_base::FileSizeConfig;
use dslp_core::Finding;
use dslp_lang_common::{file_size, FileSizeText};

const TEXT: FileSizeText = FileSizeText {
    message_tail: "Consider splitting along class or namespace boundaries.",
    fix_hint: "Move free functions or related class methods into their own translation \
               unit. Header-only libraries can split internal helpers into a `_detail.hpp`.",
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
    fn fires_above_threshold() {
        let r = run(Path::new("a.cpp"), &"x\n".repeat(8), "cpp.file-size", &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "cpp.file-size");
    }
}
