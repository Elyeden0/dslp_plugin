//! SARIF 2.1.0 export, so results can feed GitHub code scanning and other
//! SARIF consumers.

use std::collections::BTreeSet;

use dslp_core::{Issue, Severity};
use serde_json::{json, Value};

fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Low => "note",
        Severity::Medium => "warning",
        Severity::High | Severity::Critical => "error",
    }
}

fn region(issue: &Issue) -> Value {
    match issue.span {
        Some(s) => json!({
            "startLine": s.start.line,
            "startColumn": s.start.col + 1,
            "endLine": s.end.line,
            "endColumn": s.end.col + 1,
        }),
        None => json!({ "startLine": 1 }),
    }
}

pub fn sarif(issues: &[&Issue]) -> Value {
    let rules: BTreeSet<&str> = issues.iter().map(|i| i.detector.as_str()).collect();
    let results: Vec<Value> = issues
        .iter()
        .map(|i| {
            let text = match &i.fix_hint {
                Some(hint) => format!("{} {}", i.message, hint),
                None => i.message.clone(),
            };
            json!({
                "ruleId": i.detector,
                "level": level(i.severity),
                "message": { "text": text },
                "locations": [{
                    "physicalLocation": {
                        "artifactLocation": { "uri": i.file.to_string_lossy().replace('\\', "/") },
                        "region": region(i),
                    }
                }],
                "partialFingerprints": { "dslp/v3": i.fingerprint },
            })
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "dslp",
                "version": env!("CARGO_PKG_VERSION"),
                "rules": rules.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>(),
            }},
            "results": results,
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use dslp_core::{Finding, Span};
    use std::path::PathBuf;

    #[test]
    fn maps_severity_location_and_fingerprint() {
        let f = Finding::new(
            "rust.complexity",
            Severity::Medium,
            PathBuf::from("src/a.rs"),
            Some(Span::single_line(7, 4, 9)),
            "too complex",
            Some("split it".into()),
            "s",
        );
        let issue = Issue::from_new_finding(f, Utc::now());
        let v = sarif(&[&issue]);
        let r = &v["runs"][0]["results"][0];
        assert_eq!(v["version"], "2.1.0");
        assert_eq!(r["ruleId"], "rust.complexity");
        assert_eq!(r["level"], "warning");
        assert_eq!(r["message"]["text"], "too complex split it");
        let loc = &r["locations"][0]["physicalLocation"];
        assert_eq!(loc["artifactLocation"]["uri"], "src/a.rs");
        assert_eq!(loc["region"]["startLine"], 7);
        assert_eq!(loc["region"]["startColumn"], 5);
        assert_eq!(
            r["partialFingerprints"]["dslp/v3"],
            issue.fingerprint.as_str()
        );
        assert_eq!(
            v["runs"][0]["tool"]["driver"]["rules"][0]["id"],
            "rust.complexity"
        );
    }
}
