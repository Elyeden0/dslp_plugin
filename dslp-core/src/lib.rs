//! Domain types and plugin traits for noAIslop.
//!
//! This crate is dependency-light on purpose: no parsers, no I/O, no global
//! state. Everything else in the workspace depends on this.

mod finding;
mod issue;
mod plugin;
mod score;
mod severity;
mod span;

pub use finding::Finding;
pub use issue::Issue;
pub use plugin::{FileHandle, LanguagePlugin, ScanContext};
pub use score::{DetectorBreakdown, Score};
pub use severity::Severity;
pub use span::{Position, Span};
