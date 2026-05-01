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
    /// The fingerprint is `blake3(detector || "\0" || file || "\0" || start_line || "\0" || snippet)`.
    /// `snippet` lets us distinguish multiple findings on the same line, but
    /// renders us mildly fragile to reformatting — accepted v0.1 tradeoff.
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
        let fingerprint = compute_fingerprint(&detector, &file, span, snippet);
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

fn compute_fingerprint(detector: &str, file: &Path, span: Option<Span>, snippet: &str) -> String {
    let line = span.map(|s| s.start.line).unwrap_or(0);
    let mut hasher = blake3::Hasher::new();
    hasher.update(detector.as_bytes());
    hasher.update(b"\0");
    hasher.update(file.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(line.to_string().as_bytes());
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
    fn finding_serde_roundtrips() {
        let f = fixture_finding();
        let s = serde_json::to_string(&f).unwrap();
        let g: Finding = serde_json::from_str(&s).unwrap();
        assert_eq!(f, g);
    }
}
