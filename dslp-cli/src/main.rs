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
    /// Print the latest composite score (no rescan)
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
            println!(r#"{{"clean":true,"composite":{}}}"#, state.score.composite);
        } else {
            println!("No open issues. Score {:.1}", state.score.composite);
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
        println!("Score: {:.1} / 100   ({} open issue(s))", state.score.composite, state.score.finding_count);
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
        println!("No open issues. Score {:.1}", state.score.composite);
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

fn no_scan(path: &Path, json: bool) -> anyhow::Result<ExitCode> {
    if json {
        println!(r#"{{"error":"no_scan","message":"run `dslp scan` first"}}"#);
    } else {
        eprintln!("No scan found at {}/.dslp/state.json. Run `dslp scan` first.", path.display());
    }
    Ok(ExitCode::from(1))
}

fn print_score_line(s: &Score) {
    println!("Score: {:.1} / 100   ({} open issue(s))", s.composite, s.finding_count);
}

fn print_issue_line(issue: &Issue) {
    let line = issue.span.map(|s| s.start.line).unwrap_or(0);
    println!(
        "[{}] {}  {}:{}",
        severity_label(issue),
        issue.detector,
        issue.file.display(),
        line,
    );
}

fn print_issue(issue: &Issue) {
    print_issue_line(issue);
    println!("  {}", issue.message);
    if let Some(hint) = &issue.fix_hint {
        println!();
        println!("  How to fix: {}", hint);
    }
}

fn severity_label(i: &Issue) -> &'static str {
    use dslp_core::Severity::*;
    match i.severity {
        Low => "low",
        Medium => "medium",
        High => "high",
        Critical => "critical",
    }
}
