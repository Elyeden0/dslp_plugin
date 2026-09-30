use std::collections::BTreeMap;
use std::path::Path;

use dslp_core::Severity;
use serde::{Deserialize, Serialize};

/// User-facing config loaded from `.dslp/config.toml`.
///
/// Every section is optional; defaults match the design doc.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub scan: ScanConfig,
    pub detectors: DetectorsConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanConfig {
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DetectorsConfig {
    /// Detector ids to turn off; a trailing `.*` matches a whole family
    /// (`"ts.*"`).
    pub disable: Vec<String>,
    /// Per-detector severity overrides, e.g. `"rust.params" = "medium"`.
    pub severity: BTreeMap<String, Severity>,
    /// Per-language threshold overrides, keyed by plugin id
    /// (`[detectors.languages.python]`).
    pub languages: BTreeMap<String, LanguageOverride>,
    pub file_size: FileSizeConfig,
    pub complexity: ComplexityConfig,
    pub dead_pub: DeadPubConfig,
}

/// Thresholds a single language overrides; unset fields fall back to the
/// global `[detectors.file_size]` / `[detectors.complexity]` values.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LanguageOverride {
    pub file_size_threshold: Option<u32>,
    pub file_size_severity_at: Option<u32>,
    pub cyclomatic_threshold: Option<u32>,
    pub length_threshold: Option<u32>,
    pub param_threshold: Option<u32>,
    pub nesting_threshold: Option<u32>,
}

impl DetectorsConfig {
    /// File-size settings for plugin `lang`, with its overrides applied.
    pub fn file_size_for(&self, lang: &str) -> FileSizeConfig {
        let base = &self.file_size;
        let over = self.languages.get(lang);
        FileSizeConfig {
            threshold: over
                .and_then(|o| o.file_size_threshold)
                .unwrap_or(base.threshold),
            severity_at: over
                .and_then(|o| o.file_size_severity_at)
                .unwrap_or(base.severity_at),
        }
    }

    /// Complexity settings for plugin `lang`, with its overrides applied.
    pub fn complexity_for(&self, lang: &str) -> ComplexityConfig {
        let base = &self.complexity;
        let over = self.languages.get(lang);
        ComplexityConfig {
            cyclomatic_threshold: over
                .and_then(|o| o.cyclomatic_threshold)
                .unwrap_or(base.cyclomatic_threshold),
            length_threshold: over
                .and_then(|o| o.length_threshold)
                .unwrap_or(base.length_threshold),
            param_threshold: over
                .and_then(|o| o.param_threshold)
                .unwrap_or(base.param_threshold),
            nesting_threshold: over
                .and_then(|o| o.nesting_threshold)
                .unwrap_or(base.nesting_threshold),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileSizeConfig {
    pub threshold: u32,
    pub severity_at: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ComplexityConfig {
    pub cyclomatic_threshold: u32,
    pub length_threshold: u32,
    pub param_threshold: u32,
    pub nesting_threshold: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeadPubConfig {
    pub ignore_paths: Vec<String>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            exclude: vec!["target/".into(), "node_modules/".into()],
        }
    }
}

impl Default for FileSizeConfig {
    fn default() -> Self {
        Self {
            threshold: 400,
            severity_at: 600,
        }
    }
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            cyclomatic_threshold: 10,
            length_threshold: 50,
            param_threshold: 6,
            nesting_threshold: 5,
        }
    }
}

/// The default config as TOML, for `dslp init` — generated from the same
/// struct the loader reads, so it can't drift from the real defaults.
pub fn default_config_toml() -> anyhow::Result<String> {
    Ok(toml::to_string_pretty(&Config::default())?)
}

/// Load config from `<root>/.dslp/config.toml`. Missing file → defaults.
pub fn load_config(project_root: &Path) -> anyhow::Result<Config> {
    let path = project_root.join(".dslp").join("config.toml");
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)?;
    let config: Config = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("failed to parse {}: {}", path.display(), e))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmpdir(label: &str) -> std::path::PathBuf {
        let p =
            std::env::temp_dir().join(format!("dslp-base-config-{}-{}", label, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn defaults_apply_when_file_absent() {
        let dir = tmpdir("missing");
        let cfg = load_config(&dir).unwrap();
        assert_eq!(cfg.detectors.file_size.threshold, 400);
        assert_eq!(cfg.detectors.complexity.cyclomatic_threshold, 10);
    }

    #[test]
    fn default_toml_round_trips_to_defaults() {
        let parsed: Config = toml::from_str(&default_config_toml().unwrap()).unwrap();
        assert_eq!(parsed, Config::default());
    }

    #[test]
    fn disable_and_severity_overrides_parse() {
        let dir = tmpdir("overrides");
        fs::create_dir_all(dir.join(".dslp")).unwrap();
        fs::write(
            dir.join(".dslp/config.toml"),
            "[detectors]\ndisable = [\"ts.*\"]\n[detectors.severity]\n\"rust.params\" = \"high\"\n",
        )
        .unwrap();
        let cfg = load_config(&dir).unwrap();
        assert_eq!(cfg.detectors.disable, ["ts.*"]);
        assert_eq!(cfg.detectors.severity["rust.params"], Severity::High);
    }

    #[test]
    fn language_overrides_fall_back_to_globals() {
        let dir = tmpdir("lang-overrides");
        fs::create_dir_all(dir.join(".dslp")).unwrap();
        fs::write(
            dir.join(".dslp/config.toml"),
            "[detectors.complexity]\ncyclomatic_threshold = 12\n[detectors.languages.python]\ncyclomatic_threshold = 20\nfile_size_threshold = 900\n",
        )
        .unwrap();
        let d = load_config(&dir).unwrap().detectors;
        assert_eq!(d.complexity_for("python").cyclomatic_threshold, 20);
        assert_eq!(d.complexity_for("python").length_threshold, 50);
        assert_eq!(d.complexity_for("go").cyclomatic_threshold, 12);
        assert_eq!(d.file_size_for("python").threshold, 900);
        assert_eq!(d.file_size_for("python").severity_at, 600);
        assert_eq!(d.file_size_for("go").threshold, 400);
    }

    #[test]
    fn unknown_key_is_an_error() {
        let dir = tmpdir("typo");
        fs::create_dir_all(dir.join(".dslp")).unwrap();
        fs::write(
            dir.join(".dslp/config.toml"),
            "[detectors.file_size]\ntreshold = 200\n",
        )
        .unwrap();
        let err = load_config(&dir).unwrap_err();
        assert!(err.to_string().contains("treshold"), "{err}");
    }

    #[test]
    fn user_values_override_partial() {
        let dir = tmpdir("partial");
        fs::create_dir_all(dir.join(".dslp")).unwrap();
        fs::write(
            dir.join(".dslp/config.toml"),
            "[detectors.file_size]\nthreshold = 200\n",
        )
        .unwrap();
        let cfg = load_config(&dir).unwrap();
        assert_eq!(cfg.detectors.file_size.threshold, 200);
        assert_eq!(cfg.detectors.file_size.severity_at, 600); // default preserved
    }
}
