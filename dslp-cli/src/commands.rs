use std::path::Path;
use std::process::ExitCode;

use dslp_core::{Issue, LanguagePlugin};

use crate::output::{print_delta_line, print_issue, print_issue_line, print_score_line, severity_label};

/// CI gating conditions for `scan`; any violated one makes it exit 1.
pub struct ScanGate {
    pub fail_under: Option<f64>,
    pub fail_on_new: bool,
}

impl ScanGate {
    fn violation(&self, report: &dslp_engine::ScanReport) -> Option<String> {
        let strict = report.state.score.strict;
        if let Some(min) = self.fail_under.filter(|min| strict < *min) {
            return Some(format!("strict score {strict:.1} is below --fail-under {min}"));
        }
        let d = &report.delta;
        if self.fail_on_new && d.has_baseline() && d.new + d.regressed > 0 {
            return Some(format!("{} new and {} regressed issue(s) since the previous scan", d.new, d.regressed));
        }
        None
    }
}

pub fn cmd_scan(
    path: &Path,
    plugins: Vec<Box<dyn LanguagePlugin>>,
    json: bool,
    lang: Option<&str>,
    gate: ScanGate,
) -> anyhow::Result<ExitCode> {
    let selected: Vec<Box<dyn LanguagePlugin>> = match lang {
        Some(id) => plugins.into_iter().filter(|p| p.id() == id).collect(),
        None => plugins,
    };
    let report = dslp_engine::run_scan(path, &selected, lang)?;
    let score = &report.state.score;
    if json {
        let payload = serde_json::json!({
            "files_scanned": report.files_scanned,
            "total_loc": report.total_loc,
            "score": score,
            "open_count": score.open_count,
            "wontfix_count": score.wontfix_count,
            "delta": report.delta,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("Scanned {} file(s), {} LoC", report.files_scanned, report.total_loc);
        print_score_line(score);
        print_delta_line(&report.delta, score.strict);
    }
    if let Some(why) = gate.violation(&report) {
        eprintln!("error: {why}");
        return Ok(ExitCode::from(1));
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_next(path: &Path, json: bool, lang: Option<&str>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path, lang)? else {
        return no_scan(path, lang, json);
    };
    let Some(issue) = dslp_engine::next_issue(&state) else {
        if json {
            println!(r#"{{"clean":true,"lenient":{},"strict":{}}}"#, state.score.lenient, state.score.strict);
        } else {
            println!("No open issues. Lenient {:.1} / Strict {:.1}", state.score.lenient, state.score.strict);
        }
        return Ok(ExitCode::from(1));
    };
    if json {
        println!("{}", serde_json::to_string_pretty(issue)?);
    } else {
        print_issue(issue);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_score(path: &Path, json: bool, lang: Option<&str>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path, lang)? else {
        return no_scan(path, lang, json);
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&state.score)?);
    } else {
        let s = &state.score;
        println!("Lenient: {:.1} / 100", s.lenient);
        println!("Strict:  {:.1} / 100", s.strict);
        println!("Gap:     {:.1}   ({} open, {} wontfix, {} LoC)", s.gap(), s.open_count, s.wontfix_count, s.total_loc);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_findings(path: &Path, json: bool, limit: Option<usize>, lang: Option<&str>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path, lang)? else {
        return no_scan(path, lang, json);
    };
    let issues = dslp_engine::queue(&state);
    let view: Vec<&Issue> = match limit {
        Some(n) => issues.into_iter().take(n).collect(),
        None => issues,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else if view.is_empty() {
        println!("No open issues. Lenient {:.1} / Strict {:.1}", state.score.lenient, state.score.strict);
    } else {
        for issue in &view {
            print_issue_line(issue);
        }
        println!();
        println!("{} open issue(s).", view.len());
        print_score_line(&state.score);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_plan(path: &Path, json: bool, limit: Option<usize>, lang: Option<&str>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path, lang)? else {
        return no_scan(path, lang, json);
    };
    let entries = dslp_engine::plan(&state);
    let view: Vec<_> = match limit {
        Some(n) => entries.into_iter().take(n).collect(),
        None => entries,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else if view.is_empty() {
        println!("No open issues. Lenient {:.1} / Strict {:.1}", state.score.lenient, state.score.strict);
    } else {
        for entry in &view {
            println!(
                "[{} pts] {}  ({} issue(s), highest: {})",
                entry.penalty,
                entry.file.display(),
                entry.issue_count,
                severity_label(entry.highest_severity),
            );
        }
        println!();
        println!("{} file(s) with open issues.", view.len());
        print_score_line(&state.score);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_backlog(path: &Path, json: bool, lang: Option<&str>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path, lang)? else {
        return no_scan(path, lang, json);
    };
    let issues = dslp_engine::backlog(&state);
    if json {
        println!("{}", serde_json::to_string_pretty(&issues)?);
    } else if issues.is_empty() {
        println!("No wontfix issues.");
    } else {
        for issue in &issues {
            let reason = match &issue.status {
                dslp_core::Status::Wontfix { reason, .. } => reason.as_str(),
                _ => unreachable!("backlog only contains Wontfix issues"),
            };
            println!(
                "[{}] {}  {}  ({})  — {}",
                severity_label(issue.severity),
                issue.detector,
                issue.file.display(),
                crate::output::short_fp(&issue.fingerprint),
                reason,
            );
        }
        println!();
        println!("{} wontfix issue(s).", issues.len());
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_badge(path: &Path, label: &str, lenient: bool, lang: Option<&str>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path, lang)? else {
        println!("{}", serde_json::to_string_pretty(&dslp_engine::Badge::no_scan(label))?);
        return Ok(ExitCode::from(1));
    };
    let score = if lenient { state.score.lenient } else { state.score.strict };
    let badge = dslp_engine::Badge::from_score(score, label);
    println!("{}", serde_json::to_string_pretty(&badge)?);
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_resolve(path: &Path, lang: Option<&str>, fingerprint: &str, json: bool) -> anyhow::Result<ExitCode> {
    let res = dslp_engine::resolve(path, lang, fingerprint)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "resolved": res.issue.fingerprint,
                "detector": res.issue.detector,
                "score": res.score,
            }))?
        );
    } else {
        println!("Resolved {} ({})", crate::output::short_fp(&res.issue.fingerprint), res.issue.detector);
        print_score_line(&res.score);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_reopen(path: &Path, lang: Option<&str>, fingerprint: &str, json: bool) -> anyhow::Result<ExitCode> {
    let res = dslp_engine::reopen(path, lang, fingerprint)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "reopened": res.issue.fingerprint,
                "detector": res.issue.detector,
                "score": res.score,
            }))?
        );
    } else {
        println!("Reopened {} ({})", crate::output::short_fp(&res.issue.fingerprint), res.issue.detector);
        print_score_line(&res.score);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn cmd_suppress(path: &Path, lang: Option<&str>, fingerprint: &str, reason: &str, json: bool) -> anyhow::Result<ExitCode> {
    let res = dslp_engine::suppress(path, lang, fingerprint, reason)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "suppressed": res.issue.fingerprint,
                "detector": res.issue.detector,
                "reason": reason,
                "score": res.score,
            }))?
        );
    } else {
        println!("Suppressed {} ({}) — {}", crate::output::short_fp(&res.issue.fingerprint), res.issue.detector, reason);
        print_score_line(&res.score);
    }
    Ok(ExitCode::SUCCESS)
}

pub fn no_scan(path: &Path, lang: Option<&str>, json: bool) -> anyhow::Result<ExitCode> {
    let state_path = dslp_engine::state_path(path, lang);
    if json {
        println!(r#"{{"error":"no_scan","message":"run `dslp scan` first"}}"#);
    } else {
        eprintln!("No scan found at {}. Run `dslp scan` first.", state_path.display());
    }
    Ok(ExitCode::from(1))
}
