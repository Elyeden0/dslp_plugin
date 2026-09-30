use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Severity, Span};

/// A single issue produced by a detector during a scan.
///
/// `Finding` is the transient form. The engine merges these against persistent
/// state to produce [`crate::Issue`], which tracks `first_seen` / `resolved` etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub detector: String,
    pub severity: Severity,
    pub file: PathBuf,
    pub span: Option<Span>,
    pub message: String,
    pub fix_hint: Option<String>,
    pub fingerprint: String,
}

impl Finding {
    /// Construct a Finding, computing its fingerprint deterministically.
    ///
    /// The fingerprint is `blake3(detector || "\0" || file || "\0" || snippet)`.
    /// It deliberately excludes the line number and any measured value, so an
    /// issue keeps its identity when code above it moves or its metric drifts.
    /// `snippet` should name the item (function, file, ...) being flagged.
    pub fn new(
        detector: impl Into<String>,
        severity: Severity,
        file: PathBuf,
        span: Option<Span>,
        message: impl Into<String>,
        fix_hint: Option<String>,
        snippet: &str,
    ) -> Self {
        let detector = detector.into();
        let message = message.into();
        let fingerprint = compute_fingerprint(&detector, &file, snippet);
        Self {
            detector,
            severity,
            file,
            span,
            message,
            fix_hint,
            fingerprint,
        }
    }
}

impl Finding {
    /// Disambiguate the n-th (n >= 1) finding that shares a fingerprint with
    /// an earlier one in the same scan, e.g. two functions with the same name.
    pub fn with_occurrence(mut self, n: usize) -> Self {
        if n > 0 {
            let mut hasher = blake3::Hasher::new();
            hasher.update(self.fingerprint.as_bytes());
            hasher.update(b"#");
            hasher.update(n.to_string().as_bytes());
            self.fingerprint = hex::encode_short(hasher.finalize().as_bytes());
        }
        self
    }
}

fn compute_fingerprint(detector: &str, file: &Path, snippet: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(detector.as_bytes());
    hasher.update(b"\0");
    hasher.update(file.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(snippet.as_bytes());
    hex::encode_short(hasher.finalize().as_bytes())
}

mod hex {
    pub fn encode_short(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(16);
        for b in bytes.iter().take(8) {
            s.push_str(&format!("{:02x}", b));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_finding() -> Finding {
        Finding::new(
            "rust.file-size",
            Severity::Medium,
            PathBuf::from("src/foo.rs"),
            Some(Span::single_line(1, 0, 0)),
            "File too long",
            Some("Split it up".into()),
            "src/foo.rs",
        )
    }

    #[test]
    fn fingerprint_is_stable_across_constructions() {
        let a = fixture_finding();
        let b = fixture_finding();
        assert_eq!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn fingerprint_changes_with_detector() {
        let a = fixture_finding();
        let b = Finding::new(
            "rust.complexity",
            Severity::Medium,
            PathBuf::from("src/foo.rs"),
            Some(Span::single_line(1, 0, 0)),
            "File too long",
            Some("Split it up".into()),
            "src/foo.rs",
        );
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn fingerprint_changes_with_file() {
        let a = fixture_finding();
        let b = Finding::new(
            "rust.file-size",
            Severity::Medium,
            PathBuf::from("src/bar.rs"),
            Some(Span::single_line(1, 0, 0)),
            "File too long",
            Some("Split it up".into()),
            "src/bar.rs",
        );
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn fingerprint_ignores_line_number() {
        let a = fixture_finding();
        let b = Finding::new(
            "rust.file-size",
            Severity::Medium,
            PathBuf::from("src/foo.rs"),
            Some(Span::single_line(40, 0, 0)),
            "File too long",
            Some("Split it up".into()),
            "src/foo.rs",
        );
        assert_eq!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn with_occurrence_changes_fingerprint() {
        let a = fixture_finding();
        let b = a.clone().with_occurrence(1);
        assert_ne!(a.fingerprint, b.fingerprint);
        assert_eq!(a.clone().with_occurrence(0).fingerprint, a.fingerprint);
    }

    #[test]
    fn finding_serde_roundtrips() {
        let f = fixture_finding();
        let s = serde_json::to_string(&f).unwrap();
        let g: Finding = serde_json::from_str(&s).unwrap();
        assert_eq!(f, g);
    }
}
