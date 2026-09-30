//! Infrastructure for noAIslop: file discovery, config loading, parser glue.

mod config;
mod discovery;
mod parser;

pub use config::{
    default_config_toml, load_config, ComplexityConfig, Config, DeadPubConfig, DetectorsConfig,
    DuplicatesConfig, FileSizeConfig, LanguageOverride, ScanConfig, SlopConfig,
};
pub use discovery::discover_files;
pub use parser::parse;
