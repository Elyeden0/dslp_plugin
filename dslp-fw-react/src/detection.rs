use std::path::Path;

use dslp_core::FileHandle;

/// Does this project use React?
///
/// Cheap two-step heuristic:
/// 1. If `package.json` exists, inspect the merged `dependencies` +
///    `devDependencies` for any of `react`, `react-dom`, `next`, `gatsby`,
///    `remix`, `@remix-run/react`, etc.
/// 2. If `package.json` is missing or unreadable, fall back to "any `.tsx`
///    file present in the file set." TSX is React-flavored by convention;
///    plain `.ts` doesn't imply React.
pub fn is_react_project(project_root: &Path, files: &[FileHandle]) -> bool {
    if let Some(true) = check_package_json(project_root) {
        return true;
    }
    files
        .iter()
        .any(|fh| fh.path.extension().and_then(|s| s.to_str()) == Some("tsx"))
}

fn check_package_json(project_root: &Path) -> Option<bool> {
    let path = project_root.join("package.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let frameworks = [
        "react",
        "react-dom",
        "next",
        "gatsby",
        "remix",
        "@remix-run/react",
    ];
    let has = |key: &str| -> bool {
        v.get(key)
            .and_then(|m| m.as_object())
            .map(|m| frameworks.iter().any(|fw| m.contains_key(*fw)))
            .unwrap_or(false)
    };
    Some(has("dependencies") || has("devDependencies") || has("peerDependencies"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmpdir(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dslp-fw-react-detect-{}-{}",
            label,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn package_json_with_react_dep_is_detected() {
        let dir = tmpdir("react_pkg");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"react":"^18.0.0"}}"#,
        )
        .unwrap();
        assert!(is_react_project(&dir, &[]));
    }

    #[test]
    fn package_json_without_react_is_not_detected() {
        let dir = tmpdir("no_react");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"lodash":"^4.0.0"}}"#,
        )
        .unwrap();
        assert!(!is_react_project(&dir, &[]));
    }

    #[test]
    fn tsx_files_imply_react_when_no_package_json() {
        let dir = tmpdir("tsx_only");
        let files = vec![FileHandle {
            path: PathBuf::from("App.tsx"),
            source: "".into(),
        }];
        assert!(is_react_project(&dir, &files));
    }

    #[test]
    fn pure_ts_files_do_not_imply_react() {
        let dir = tmpdir("ts_only");
        let files = vec![FileHandle {
            path: PathBuf::from("util.ts"),
            source: "".into(),
        }];
        assert!(!is_react_project(&dir, &files));
    }

    #[test]
    fn next_dep_counts_as_react() {
        let dir = tmpdir("next");
        std::fs::write(
            dir.join("package.json"),
            r#"{"dependencies":{"next":"^14.0.0"}}"#,
        )
        .unwrap();
        assert!(is_react_project(&dir, &[]));
    }
}
