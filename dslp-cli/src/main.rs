use std::process::ExitCode;

use clap::Parser;
use dslp_core::LanguagePlugin;

mod cli;
mod commands;
mod output;

use cli::{Cli, Command, MutateCommand, QueryCommand};
use commands::{
    cmd_backlog, cmd_badge, cmd_findings, cmd_next, cmd_plan, cmd_resolve, cmd_scan, cmd_score,
    cmd_suppress,
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

    match dispatch(cli.command, plugins) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {:#}", e);
            ExitCode::from(2)
        }
    }
}

fn dispatch(command: Command, plugins: Vec<Box<dyn LanguagePlugin>>) -> anyhow::Result<ExitCode> {
    match command {
        Command::Scan { path_arg, path_flag, json, lang } => {
            cmd_scan(&resolve_path(path_arg, path_flag), plugins, json, lang.as_deref())
        }
        Command::Query(q) => dispatch_query(q),
        Command::Mutate(m) => dispatch_mutate(m),
    }
}

fn dispatch_query(command: QueryCommand) -> anyhow::Result<ExitCode> {
    match command {
        QueryCommand::Next { path_arg, path_flag, json, lang } => {
            cmd_next(&resolve_path(path_arg, path_flag), json, lang.as_deref())
        }
        QueryCommand::Score { path_arg, path_flag, json, lang } => {
            cmd_score(&resolve_path(path_arg, path_flag), json, lang.as_deref())
        }
        QueryCommand::Findings { path_arg, path_flag, json, limit, lang } => {
            cmd_findings(&resolve_path(path_arg, path_flag), json, limit, lang.as_deref())
        }
        QueryCommand::Plan { path_arg, path_flag, json, limit, lang } => {
            cmd_plan(&resolve_path(path_arg, path_flag), json, limit, lang.as_deref())
        }
        QueryCommand::Backlog { path_arg, path_flag, json, lang } => {
            cmd_backlog(&resolve_path(path_arg, path_flag), json, lang.as_deref())
        }
        QueryCommand::Badge { path_arg, path_flag, label, lenient, lang } => {
            cmd_badge(&resolve_path(path_arg, path_flag), &label, lenient, lang.as_deref())
        }
    }
}

fn dispatch_mutate(command: MutateCommand) -> anyhow::Result<ExitCode> {
    match command {
        MutateCommand::Resolve { fingerprint, path_arg, path_flag, json, lang } => {
            cmd_resolve(&resolve_path(path_arg, path_flag), lang.as_deref(), &fingerprint, json)
        }
        MutateCommand::Suppress { fingerprint, reason, path_arg, path_flag, json, lang } => {
            cmd_suppress(&resolve_path(path_arg, path_flag), lang.as_deref(), &fingerprint, &reason, json)
        }
    }
}

/// The bare positional path (`dslp scan ./project`) takes precedence over
/// `--path`, which itself defaults to the current directory.
fn resolve_path(path_arg: Option<std::path::PathBuf>, path_flag: std::path::PathBuf) -> std::path::PathBuf {
    path_arg.unwrap_or(path_flag)
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
