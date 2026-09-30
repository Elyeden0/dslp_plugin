//! Apply `[detectors] disable` / `[detectors.severity]` from config to a
//! scan's findings. Done in the engine so every plugin gets it for free.

use dslp_base::DetectorsConfig;
use dslp_core::Finding;

fn matches(pattern: &str, detector: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => detector.starts_with(prefix),
        None => detector == pattern,
    }
}

pub fn apply(findings: Vec<Finding>, cfg: &DetectorsConfig) -> Vec<Finding> {
    findings
        .into_iter()
        .filter(|f| !cfg.disable.iter().any(|p| matches(p, &f.detector)))
        .map(|mut f| {
            if let Some(sev) = cfg.severity.get(&f.detector) {
                f.severity = *sev;
            }
            f
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use dslp_core::Severity;
    use std::path::PathBuf;

    fn finding(detector: &str) -> Finding {
        Finding::new(detector, Severity::Low, PathBuf::from("a"), None, "m", None, "s")
    }

    #[test]
    fn disable_matches_exact_ids_and_families() {
        let cfg = DetectorsConfig {
            disable: vec!["ts.*".into(), "rust.params".into()],
            ..Default::default()
        };
        let kept = apply(
            vec![finding("ts.length"), finding("ts.empty-catch"), finding("rust.params"), finding("rust.length")],
            &cfg,
        );
        let ids: Vec<_> = kept.iter().map(|f| f.detector.as_str()).collect();
        assert_eq!(ids, ["rust.length"]);
    }

    #[test]
    fn severity_override_changes_severity_not_fingerprint() {
        let mut cfg = DetectorsConfig::default();
        cfg.severity.insert("rust.length".into(), Severity::High);
        let before = finding("rust.length");
        let after = apply(vec![before.clone()], &cfg).remove(0);
        assert_eq!(after.severity, Severity::High);
        assert_eq!(after.fingerprint, before.fingerprint);
    }
}
