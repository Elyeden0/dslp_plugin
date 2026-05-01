//! Infrastructure for noAIslop: file discovery, config loading, parser glue.

mod config;
mod discovery;
mod parser;

pub use config::{
    load_config, ComplexityConfig, Config, DeadPubConfig, DetectorsConfig, FileSizeConfig,
    ScanConfig,
};
pub use discovery::discover_files;
pub use parser::parse;
