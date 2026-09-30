use dslp_core::{Issue, Score};

pub fn print_score_line(s: &Score) {
    if s.wontfix_count > 0 {
        println!(
            "Lenient {:.1} / Strict {:.1} (gap {:.1})   {} open · {} wontfix",
            s.lenient,
            s.strict,
            s.gap(),
            s.open_count,
            s.wontfix_count
        );
    } else {
        println!(
            "Score: {:.1} / 100   ({} open issue(s))",
            s.lenient, s.open_count
        );
    }
}

/// One-line "what this scan changed"; silent on the first scan (no baseline).
pub fn print_delta_line(d: &dslp_engine::ScanDelta, strict: f64) {
    let Some(before) = d.previous_strict else {
        return;
    };
    println!(
        "Since last scan: +{} new, {} fixed, {} regressed (strict {:.1} → {:.1})",
        d.new, d.fixed, d.regressed, before, strict
    );
}

pub fn print_issue_line(issue: &Issue) {
    let line = issue.span.map(|s| s.start.line).unwrap_or(0);
    println!(
        "[{}] {}  {}:{}  ({})",
        severity_label(issue.severity),
        issue.detector,
        issue.file.display(),
        line,
        short_fp(&issue.fingerprint),
    );
}

pub fn print_issue(issue: &Issue) {
    print_issue_line(issue);
    println!("  {}", issue.message);
    if let Some(hint) = &issue.fix_hint {
        println!();
        println!("  How to fix: {}", hint);
    }
    println!();
    println!(
        "  When done: `dslp resolve {}`  (or `dslp suppress {} --reason \"...\"`)",
        short_fp(&issue.fingerprint),
        short_fp(&issue.fingerprint)
    );
}

pub fn short_fp(fp: &str) -> &str {
    // Fingerprints are 16 hex chars; the first 8 are plenty to address one.
    &fp[..fp.len().min(8)]
}

pub fn severity_label(s: dslp_core::Severity) -> &'static str {
    use dslp_core::Severity::*;
    match s {
        Low => "low",
        Medium => "medium",
        High => "high",
        Critical => "critical",
    }
}
