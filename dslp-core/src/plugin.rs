use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::Finding;

/// Anything a plugin needs from the engine during a scan.
///
/// Kept minimal in v0.1 — just project root and a config blob. The blob is
/// `serde_json::Value` so each plugin can parse out its own keys without
/// `dslp-core` depending on the union of all plugin config schemas.
pub struct ScanContext<'a> {
    pub project_root: &'a Path,
    pub config: &'a serde_json::Value,
}

/// One file as the engine hands it to a plugin. The plugin owns parsing
/// and any caching it wants to do internally.
#[derive(Debug, Clone)]
pub struct FileHandle {
    pub path: PathBuf,
    pub source: Arc<str>,
}

/// A language plugin: claims file extensions and runs detectors against them.
pub trait LanguagePlugin: Send + Sync {
    /// Stable identifier, e.g. `"rust"`.
    fn id(&self) -> &'static str;

    /// File extensions claimed by this plugin (no leading dot).
    fn extensions(&self) -> &'static [&'static str];

    /// Run all of this plugin's detectors against `files`, returning a flat
    /// list of findings. Dedup, ranking, and scoring are the engine's job.
    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding>;
}
