use std::process::ExitCode;

use clap::Parser;
use dslp_core::LanguagePlugin;

mod cli;
mod commands;
mod output;

use cli::{Cli, Command, MutateCommand, QueryCommand};
use commands::{
    cmd_backlog, cmd_badge, cmd_findings, cmd_init, cmd_next, cmd_plan, cmd_reopen, cmd_resolve,
    cmd_scan, cmd_score, cmd_show, cmd_suppress, ScanGate,
};

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

    if let Err(e) = validate_lang(cli.command.target().lang(), &plugins) {
        eprintln!("error: {:#}", e);
        return ExitCode::from(2);
    }
    match dispatch(cli.command, plugins) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::from(2)
        }
    }
}

/// Fail fast on an unknown `--lang`, for every command (not just `scan`).
fn validate_lang(lang: Option<&str>, plugins: &[Box<dyn LanguagePlugin>]) -> anyhow::Result<()> {
    let Some(id) = lang else { return Ok(()) };
    if plugins.iter().any(|p| p.id() == id) {
        return Ok(());
    }
    let known: Vec<&str> = plugins.iter().map(|p| p.id()).collect();
    anyhow::bail!("unknown --lang '{id}'. Valid values: {}", known.join(", "))
}

fn dispatch(command: Command, plugins: Vec<Box<dyn LanguagePlugin>>) -> anyhow::Result<ExitCode> {
    match command {
        Command::Scan {
            target,
            json,
            fail_under,
            fail_on_new,
        } => {
            let gate = ScanGate {
                fail_under,
                fail_on_new,
            };
            cmd_scan(&target.path(), plugins, json, target.lang(), gate)
        }
        Command::Init { target } => cmd_init(&target.path()),
        Command::Query(q) => dispatch_query(q),
        Command::Mutate(m) => dispatch_mutate(m),
    }
}

fn dispatch_query(command: QueryCommand) -> anyhow::Result<ExitCode> {
    match command {
        QueryCommand::Show {
            fingerprint,
            target,
            json,
        } => cmd_show(&target.path(), target.lang(), &fingerprint, json),
        QueryCommand::Next { target, json } => cmd_next(&target.path(), json, target.lang()),
        QueryCommand::Score { target, json } => cmd_score(&target.path(), json, target.lang()),
        QueryCommand::Findings {
            target,
            json,
            sarif,
            limit,
        } => cmd_findings(&target.path(), json, sarif, limit, target.lang()),
        QueryCommand::Plan {
            target,
            json,
            limit,
        } => cmd_plan(&target.path(), json, limit, target.lang()),
        QueryCommand::Backlog { target, json } => cmd_backlog(&target.path(), json, target.lang()),
        QueryCommand::Badge {
            target,
            label,
            lenient,
        } => cmd_badge(&target.path(), &label, lenient, target.lang()),
    }
}

fn dispatch_mutate(command: MutateCommand) -> anyhow::Result<ExitCode> {
    match command {
        MutateCommand::Resolve {
            fingerprint,
            target,
            json,
        } => cmd_resolve(&target.path(), target.lang(), &fingerprint, json),
        MutateCommand::Reopen {
            fingerprint,
            target,
            json,
        } => cmd_reopen(&target.path(), target.lang(), &fingerprint, json),
        MutateCommand::Suppress {
            fingerprint,
            reason,
            target,
            json,
        } => cmd_suppress(&target.path(), target.lang(), &fingerprint, &reason, json),
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
