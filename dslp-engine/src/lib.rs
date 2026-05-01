//! Scan / score / queue / state pipeline for noAIslop.

mod queue;
mod scan;
mod scoring;
mod state;

pub use queue::{next_issue, queue};
pub use scan::{run_scan, ScanReport};
pub use scoring::score_issues;
pub use state::{load, merge_issues, save, state_path, State, STATE_VERSION};
