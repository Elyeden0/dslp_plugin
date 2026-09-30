use std::path::Path;

/// Does `package.json` declare a public entry point (`main`, `module`,
/// `exports`, `types`)? Such a package is a library: its exports are its API.
pub fn is_library(project_root: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(project_root.join("package.json")) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    ["main", "module", "exports", "types", "typings"]
        .iter()
        .any(|k| v.get(k).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(label: &str, package_json: Option<&str>) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("dslp-ts-detect-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        if let Some(text) = package_json {
            std::fs::write(dir.join("package.json"), text).unwrap();
        }
        dir
    }

    #[test]
    fn entry_point_fields_mark_a_library() {
        assert!(is_library(&project(
            "main",
            Some(r#"{"main":"dist/index.js"}"#)
        )));
        assert!(is_library(&project(
            "exports",
            Some(r#"{"exports":{".":"./a.js"}}"#)
        )));
        assert!(!is_library(&project(
            "app",
            Some(r#"{"dependencies":{"react":"1"}}"#)
        )));
        assert!(!is_library(&project("none", None)));
    }
}
