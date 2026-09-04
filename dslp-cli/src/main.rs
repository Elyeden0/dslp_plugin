use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use dslp_core::{Issue, LanguagePlugin, Score};

#[derive(Parser, Debug)]
#[command(
    name = "dslp",
    version,
    about = "noAIslop — analyze a codebase, score it, queue what to fix",
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Scan a project: detect issues, write state to .dslp/state.json
    Scan {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Emit machine-readable JSON to stdout instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show the highest-priority open issue from the last scan
    Next {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Print the latest score: lenient / strict / gap (no rescan)
    Score {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List all open issues in priority order
    Findings {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Cap the number of issues shown (default: all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Group open issues by file, ranked by total severity penalty — a batch triage plan
    Plan {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Cap the number of files shown (default: all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// List wontfix (suppressed) issues — the standing-decision backlog
    Backlog {
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Mark an issue fixed (optimistic — a later scan reopens it if it's still there)
    Resolve {
        /// Issue fingerprint (a unique prefix is enough)
        fingerprint: String,
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Dismiss an issue as wontfix (still counts toward the strict score)
    Suppress {
        /// Issue fingerprint (a unique prefix is enough)
        fingerprint: String,
        /// Why this issue won't be fixed (required)
        #[arg(long)]
        reason: String,
        #[arg(long, default_value = ".")]
        path: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

fn main() -> ExitCode {
    reset_sigpipe();
    let cli = Cli::parse();
    let plugins: Vec<Box<dyn LanguagePlugin>> = vec![
        Box::new(dslp_lang_rust::RustPlugin::new()),
        Box::new(dslp_lang_ts::TsPlugin::new()),
        Box::new(dslp_lang_go::GoPlugin::new()),
        Box::new(dslp_lang_python::PythonPlugin::new()),
        Box::new(dslp_lang_c::CPlugin::new()),
        Box::new(dslp_lang_cpp::CppPlugin::new()),
        Box::new(dslp_fw_react::ReactPlugin::new()),
        Box::new(dslp_fw_nextjs::NextjsPlugin::new()),
        Box::new(dslp_fw_nuxt::NuxtPlugin::new()),
    ];

    let result = match cli.command {
        Command::Scan { path, json } => cmd_scan(&path, &plugins, json),
        Command::Next { path, json } => cmd_next(&path, json),
        Command::Score { path, json } => cmd_score(&path, json),
        Command::Findings { path, json, limit } => cmd_findings(&path, json, limit),
        Command::Plan { path, json, limit } => cmd_plan(&path, json, limit),
        Command::Backlog { path, json } => cmd_backlog(&path, json),
        Command::Resolve { fingerprint, path, json } => cmd_resolve(&path, &fingerprint, json),
        Command::Suppress { fingerprint, reason, path, json } => {
            cmd_suppress(&path, &fingerprint, &reason, json)
        }
    };

    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::from(2)
        }
    }
}

/// Restore the OS-default disposition for SIGPIPE. Rust's runtime installs
/// `SIG_IGN`, which turns a closed downstream pipe (`dslp ... | head`) into an
/// `ErrorKind::BrokenPipe` that `println!` unwraps into a panic. The default
/// disposition terminates the process by signal instead — the quiet behavior
/// every other Unix filter has.
#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: called once at startup before any threads exist, with a valid
    // handler constant; `signal` itself is async-signal-safe.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn cmd_scan(path: &Path, plugins: &[Box<dyn LanguagePlugin>], json: bool) -> anyhow::Result<ExitCode> {
    let report = dslp_engine::run_scan(path, plugins)?;
    let score = &report.state.score;
    if json {
        let payload = serde_json::json!({
            "files_scanned": report.files_scanned,
            "total_loc": report.total_loc,
            "score": score,
            "open_count": score.open_count,
            "wontfix_count": score.wontfix_count,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("Scanned {} file(s), {} LoC", report.files_scanned, report.total_loc);
        print_score_line(score);
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_next(path: &Path, json: bool) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path)? else {
        return no_scan(path, json);
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

fn cmd_score(path: &Path, json: bool) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path)? else {
        return no_scan(path, json);
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

fn cmd_findings(path: &Path, json: bool, limit: Option<usize>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path)? else {
        return no_scan(path, json);
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

fn cmd_plan(path: &Path, json: bool, limit: Option<usize>) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path)? else {
        return no_scan(path, json);
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

fn cmd_backlog(path: &Path, json: bool) -> anyhow::Result<ExitCode> {
    let Some(state) = dslp_engine::load(path)? else {
        return no_scan(path, json);
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
                short_fp(&issue.fingerprint),
                reason,
            );
        }
        println!();
        println!("{} wontfix issue(s).", issues.len());
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_resolve(path: &Path, fingerprint: &str, json: bool) -> anyhow::Result<ExitCode> {
    let res = dslp_engine::resolve(path, fingerprint)?;
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
        println!("Resolved {} ({})", short_fp(&res.issue.fingerprint), res.issue.detector);
        print_score_line(&res.score);
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_suppress(path: &Path, fingerprint: &str, reason: &str, json: bool) -> anyhow::Result<ExitCode> {
    let res = dslp_engine::suppress(path, fingerprint, reason)?;
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
        println!("Suppressed {} ({}) — {}", short_fp(&res.issue.fingerprint), res.issue.detector, reason);
        print_score_line(&res.score);
    }
    Ok(ExitCode::SUCCESS)
}

fn no_scan(path: &Path, json: bool) -> anyhow::Result<ExitCode> {
    if json {
        println!(r#"{{"error":"no_scan","message":"run `dslp scan` first"}}"#);
    } else {
        eprintln!("No scan found at {}/.dslp/state.json. Run `dslp scan` first.", path.display());
    }
    Ok(ExitCode::from(1))
}

fn print_score_line(s: &Score) {
    if s.wontfix_count > 0 {
        println!(
            "Lenient {:.1} / Strict {:.1} (gap {:.1})   {} open · {} wontfix",
            s.lenient, s.strict, s.gap(), s.open_count, s.wontfix_count
        );
    } else {
        println!("Score: {:.1} / 100   ({} open issue(s))", s.lenient, s.open_count);
    }
}

fn print_issue_line(issue: &Issue) {
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

fn print_issue(issue: &Issue) {
    print_issue_line(issue);
    println!("  {}", issue.message);
    if let Some(hint) = &issue.fix_hint {
        println!();
        println!("  How to fix: {}", hint);
    }
    println!();
    println!("  When done: `dslp resolve {}`  (or `dslp suppress {} --reason \"...\"`)", short_fp(&issue.fingerprint), short_fp(&issue.fingerprint));
}

fn short_fp(fp: &str) -> &str {
    // Fingerprints are 16 hex chars; the first 8 are plenty to address one.
    &fp[..fp.len().min(8)]
}

fn severity_label(s: dslp_core::Severity) -> &'static str {
    use dslp_core::Severity::*;
    match s {
        Low => "low",
        Medium => "medium",
        High => "high",
        Critical => "critical",
    }
}
