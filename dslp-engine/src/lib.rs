//! Scan / score / queue / state pipeline for noAIslop.

mod ops;
mod queue;
mod scan;
mod scoring;
mod state;

pub use ops::{resolve, suppress, OpResult};
pub use queue::{next_issue, queue};
pub use scan::{run_scan, ScanReport};
pub use scoring::score_issues;
pub use state::{find_by_prefix, load, merge_issues, save, state_path, State, STATE_VERSION};
