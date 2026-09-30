//! `dslp-ignore` comments: a finding whose start line, or the line above it,
//! carries `dslp-ignore <detector>[, <detector>...][: reason]` is dismissed as
//! wontfix. It is deliberately *not* dropped: like `dslp suppress`, it still
//! counts toward the strict score, so inline ignores can't game the number.

use std::collections::HashMap;
use std::path::PathBuf;

use dslp_core::Finding;

use crate::overrides::matches;

const MARKER: &str = "dslp-ignore";

/// Fingerprint -> reason for every finding covered by an inline ignore.
pub fn ignored(findings: &[Finding], sources: &HashMap<PathBuf, &str>) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for f in findings {
        let (Some(span), Some(source)) = (f.span, sources.get(&f.file)) else {
            continue;
        };
        let lines: Vec<&str> = source.lines().collect();
        let at = span.start.line as usize;
        let candidates = [at.checked_sub(1), at.checked_sub(2)];
        let reason = candidates
            .into_iter()
            .flatten()
            .filter_map(|i| lines.get(i))
            .find_map(|line| directive_for(line, &f.detector));
        if let Some(reason) = reason {
            out.insert(f.fingerprint.clone(), reason);
        }
    }
    out
}

/// The reason if `line` holds a `dslp-ignore` naming `detector`.
fn directive_for(line: &str, detector: &str) -> Option<String> {
    let rest = &line[line.find(MARKER)? + MARKER.len()..];
    let (ids, reason) = rest.split_once(':').unwrap_or((rest, ""));
    let named = ids
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .any(|pat| matches(pat, detector));
    named.then(|| {
        let reason = reason
            .trim()
            .trim_end_matches("*/")
            .trim_end_matches("-->")
            .trim();
        if reason.is_empty() {
            "inline dslp-ignore".to_string()
        } else {
            format!("inline: {reason}")
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids_and_reason() {
        assert_eq!(
            directive_for("// dslp-ignore rust.params: wire format", "rust.params").unwrap(),
            "inline: wire format"
        );
        assert_eq!(
            directive_for("# dslp-ignore rust.a, rust.b", "rust.b").unwrap(),
            "inline dslp-ignore"
        );
        assert_eq!(
            directive_for("/* dslp-ignore ts.*: legacy */", "ts.length").unwrap(),
            "inline: legacy"
        );
        assert!(directive_for("// dslp-ignore rust.a", "rust.b").is_none());
        assert!(directive_for("// nothing here", "rust.a").is_none());
    }

    #[test]
    fn covers_the_finding_line_and_the_line_above_only() {
        let src = "fn a() {}\n// dslp-ignore d\nfn b() {}\nfn c() {}\n";
        let mk = |line| {
            Finding::new(
                "d",
                dslp_core::Severity::Low,
                PathBuf::from("a.rs"),
                Some(dslp_core::Span::single_line(line, 0, 0)),
                "m",
                None,
                &line.to_string(),
            )
        };
        let findings = [mk(1), mk(2), mk(3), mk(4)];
        let sources = HashMap::from([(PathBuf::from("a.rs"), src)]);
        let got = ignored(&findings, &sources);
        // line 1: directive is on the line below, not above; 2 and 3 are covered; 4 is not.
        assert_eq!(got.len(), 2);
        assert!(
            got.contains_key(&findings[1].fingerprint)
                && got.contains_key(&findings[2].fingerprint)
        );
    }
}
