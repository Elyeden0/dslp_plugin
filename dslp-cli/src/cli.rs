use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "dslp",
    version,
    about = "noAIslop — analyze a codebase, score it, queue what to fix",
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scan a project: detect issues, write state to .dslp/state.json
    Scan {
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        /// Emit machine-readable JSON to stdout instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Restrict to one language/framework plugin (e.g. `go`, `typescript`, `react`) —
        /// scores and tracks that language independently in `.dslp/state-<lang>.json`,
        /// for monorepos with more than one language under the same --path
        #[arg(long)]
        lang: Option<String>,
    },
    #[command(flatten)]
    Query(QueryCommand),
    #[command(flatten)]
    Mutate(MutateCommand),
}

/// Read-only commands: inspect the last scan's state without changing it.
#[derive(Subcommand, Debug)]
pub enum QueryCommand {
    /// Show the highest-priority open issue from the last scan
    Next {
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long)]
        lang: Option<String>,
    },
    /// Print the latest score: lenient / strict / gap (no rescan)
    Score {
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long)]
        lang: Option<String>,
    },
    /// List all open issues in priority order
    Findings {
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Cap the number of issues shown (default: all)
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        lang: Option<String>,
    },
    /// Group open issues by file, ranked by total severity penalty — a batch triage plan
    Plan {
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Cap the number of files shown (default: all)
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        lang: Option<String>,
    },
    /// List wontfix (suppressed) issues — the standing-decision backlog
    Backlog {
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long)]
        lang: Option<String>,
    },
    /// Emit a shields.io endpoint-badge payload for the score (strict by default — see Scoring)
    Badge {
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        /// Badge label text
        #[arg(long, default_value = "dslp")]
        label: String,
        /// Use the lenient score instead of strict (strict is the anti-gaming default)
        #[arg(long, default_value_t = false)]
        lenient: bool,
        #[arg(long)]
        lang: Option<String>,
    },
}

/// Commands that change an issue's status in the tracked state.
#[derive(Subcommand, Debug)]
pub enum MutateCommand {
    /// Mark an issue fixed (optimistic — a later scan reopens it if it's still there)
    Resolve {
        /// Issue fingerprint (a unique prefix is enough)
        fingerprint: String,
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long)]
        lang: Option<String>,
    },
    /// Dismiss an issue as wontfix (still counts toward the strict score)
    Suppress {
        /// Issue fingerprint (a unique prefix is enough)
        fingerprint: String,
        /// Why this issue won't be fixed (required)
        #[arg(long)]
        reason: String,
        /// Project to analyze (defaults to the current directory)
        #[arg(value_name = "PATH")]
        path_arg: Option<PathBuf>,
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path_flag: PathBuf,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long)]
        lang: Option<String>,
    },
}
