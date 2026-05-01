use std::path::Path;

/// Does this project use Nuxt? Strict: `package.json` must declare `nuxt`
/// as a dependency. There's no useful file-extension fallback (.vue files
/// belong to plain Vue projects too).
pub fn is_nuxt_project(project_root: &Path) -> bool {
    let path = project_root.join("package.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|key| {
            v.get(*key)
                .and_then(|m| m.as_object())
                .map(|m| m.contains_key("nuxt"))
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dslp-fw-nuxt-detect-{}-{}",
            label,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn nuxt_in_dependencies_detected() {
        let dir = tmpdir("nuxt_dep");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"nuxt":"^3.0.0"}}"#,
        )
        .unwrap();
        assert!(is_nuxt_project(&dir));
    }

    #[test]
    fn vue_only_is_not_nuxt() {
        let dir = tmpdir("vue_only");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"vue":"^3.0.0"}}"#,
        )
        .unwrap();
        assert!(!is_nuxt_project(&dir));
    }

    #[test]
    fn nuxt_in_devdependencies_detected() {
        let dir = tmpdir("nuxt_dev");
        std::fs::write(
            dir.join("package.json"),
            r#"{"devDependencies":{"nuxt":"^3.0.0"}}"#,
        )
        .unwrap();
        assert!(is_nuxt_project(&dir));
    }
}
