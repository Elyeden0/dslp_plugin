use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "dslp",
    version,
    about = "noAIslop — analyze a codebase, score it, queue what to fix",
    override_usage = "dslp <COMMAND> [PATH]",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Where to look and which plugin to scope to; shared by every command.
#[derive(Args, Debug)]
pub struct Target {
    /// Project to analyze (defaults to the current directory)
    #[arg(value_name = "PATH")]
    path_arg: Option<PathBuf>,
    #[arg(long = "path", value_name = "PATH", conflicts_with = "path_arg")]
    path_flag: Option<PathBuf>,
    /// Restrict to one language/framework plugin (e.g. `go`, `typescript`, `react`) —
    /// scores and tracks that language independently in `.dslp/state-<lang>.json`,
    /// for monorepos with more than one language under the same --path
    #[arg(long)]
    pub lang: Option<String>,
}

impl Target {
    pub fn path(&self) -> PathBuf {
        self.path_arg
            .clone()
            .or_else(|| self.path_flag.clone())
            .unwrap_or_else(|| PathBuf::from("."))
    }

    pub fn lang(&self) -> Option<&str> {
        self.lang.as_deref()
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scan a project: detect issues, write state to .dslp/state.json
    Scan {
        #[command(flatten)]
        target: Target,
        /// Emit machine-readable JSON to stdout instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Exit 1 if the strict score is below this value (0-100), for CI gating
        #[arg(long, value_name = "SCORE")]
        fail_under: Option<f64>,
        /// Exit 1 if this scan found new or regressed issues since the previous
        /// scan (never fails the first scan, which has nothing to compare to)
        #[arg(long, default_value_t = false)]
        fail_on_new: bool,
    },
    /// Write a starter .dslp/config.toml and ignore .dslp/ in .gitignore
    Init {
        #[command(flatten)]
        target: Target,
    },
    #[command(flatten)]
    Query(QueryCommand),
    #[command(flatten)]
    Mutate(MutateCommand),
}

impl Command {
    pub fn target(&self) -> &Target {
        match self {
            Command::Scan { target, .. } => target,
            Command::Init { target } => target,
            Command::Query(q) => match q {
                QueryCommand::Show { target, .. }
                | QueryCommand::Next { target, .. }
                | QueryCommand::Score { target, .. }
                | QueryCommand::Findings { target, .. }
                | QueryCommand::Plan { target, .. }
                | QueryCommand::Backlog { target, .. }
                | QueryCommand::Badge { target, .. } => target,
            },
            Command::Mutate(m) => match m {
                MutateCommand::Resolve { target, .. }
                | MutateCommand::Suppress { target, .. }
                | MutateCommand::Reopen { target, .. } => target,
            },
        }
    }
}

/// Read-only commands: inspect the last scan's state without changing it.
#[derive(Subcommand, Debug)]
pub enum QueryCommand {
    /// Show one issue in full, whatever its status (open, resolved or wontfix)
    Show {
        /// Issue fingerprint (a unique prefix is enough)
        fingerprint: String,
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show the highest-priority open issue from the last scan
    Next {
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Print the latest score: lenient / strict / gap (no rescan)
    Score {
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List all open issues in priority order
    Findings {
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Emit SARIF 2.1.0 (for GitHub code scanning and other SARIF tools)
        #[arg(long, default_value_t = false, conflicts_with = "json")]
        sarif: bool,
        /// Cap the number of issues shown (default: all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Group open issues by file, ranked by total severity penalty — a batch triage plan
    Plan {
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Cap the number of files shown (default: all)
        #[arg(long)]
        limit: Option<usize>,
    },
    /// List wontfix (suppressed) issues — the standing-decision backlog
    Backlog {
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Emit a shields.io endpoint-badge payload for the score (strict by default — see Scoring)
    Badge {
        #[command(flatten)]
        target: Target,
        /// Badge label text
        #[arg(long, default_value = "dslp")]
        label: String,
        /// Use the lenient score instead of strict (strict is the anti-gaming default)
        #[arg(long, default_value_t = false)]
        lenient: bool,
    },
}

/// Commands that change an issue's status in the tracked state.
#[derive(Subcommand, Debug)]
pub enum MutateCommand {
    /// Mark an issue fixed (optimistic — a later scan reopens it if it's still there)
    Resolve {
        /// Issue fingerprint (a unique prefix is enough)
        fingerprint: String,
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Put a resolved or wontfix issue back in the queue
    Reopen {
        /// Issue fingerprint (a unique prefix is enough)
        fingerprint: String,
        #[command(flatten)]
        target: Target,
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
        #[command(flatten)]
        target: Target,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}
