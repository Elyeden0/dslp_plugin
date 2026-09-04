//! Scan / score / queue / state pipeline for noAIslop.

mod backlog;
mod badge;
mod ops;
mod plan;
mod queue;
mod scan;
mod scoring;
mod state;

pub use backlog::backlog;
pub use badge::Badge;
pub use ops::{resolve, suppress, OpResult};
pub use plan::{plan, PlanEntry};
pub use queue::{next_issue, queue};
pub use scan::{run_scan, ScanReport};
pub use scoring::score_issues;
pub use state::{find_by_prefix, load, merge_issues, save, state_path, State, STATE_VERSION};
