use std::path::Path;

/// Does this project use Next.js?
///
/// Strict: `package.json` must declare `next` as a dependency. We don't
/// fall back to file-extension heuristics — Next.js is always declared
/// explicitly, and a `.tsx` file alone could be plain React.
pub fn is_nextjs_project(project_root: &Path) -> bool {
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
                .map(|m| m.contains_key("next"))
                .unwrap_or(false)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dslp-fw-nextjs-detect-{}-{}",
            label,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn next_in_dependencies_detected() {
        let dir = tmpdir("dep");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"next":"^14.0.0"}}"#,
        )
        .unwrap();
        assert!(is_nextjs_project(&dir));
    }

    #[test]
    fn react_only_is_not_nextjs() {
        let dir = tmpdir("react_only");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"react":"^18.0.0"}}"#,
        )
        .unwrap();
        assert!(!is_nextjs_project(&dir));
    }

    #[test]
    fn missing_package_json_is_not_nextjs() {
        let dir = tmpdir("no_pkg");
        assert!(!is_nextjs_project(&dir));
    }
}
