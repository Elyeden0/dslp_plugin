use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

/// Walk `root` honoring `.gitignore`, plus any user-supplied `exclude` paths
/// (directory or file prefixes, matched component-wise against the path
/// relative to `root`, so `src` excludes `src/a.rs` but not `src2/a.rs`). Returns files whose extension
/// matches one of `extensions` (without the leading dot).
pub fn discover_files(
    root: &Path,
    extensions: &[&str],
    excludes: &[String],
) -> Vec<PathBuf> {
    let mut walker = WalkBuilder::new(root);
    walker
        .hidden(false) // .dotfiles are fine to surface; gitignore still hides .git
        .require_git(false); // honor .gitignore even without a .git dir (handy for tests + new projects)
    let walker = walker.build();

    walker
        .filter_map(|res| res.ok())
        .filter(|entry| entry.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|entry| entry.into_path())
        .filter(|p| has_matching_extension(p, extensions))
        .filter(|p| !is_excluded(p, root, excludes))
        .collect()
}

fn has_matching_extension(path: &Path, extensions: &[&str]) -> bool {
    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
        return false;
    };
    extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
}

fn is_excluded(path: &Path, root: &Path, excludes: &[String]) -> bool {
    if excludes.is_empty() {
        return false;
    }
    let rel = match path.strip_prefix(root) {
        Ok(r) => r,
        Err(_) => path,
    };
    excludes
        .iter()
        .any(|prefix| rel.starts_with(prefix.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn make_tree(base: &Path, files: &[(&str, &str)]) {
        for (rel, contents) in files {
            let p = base.join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            let mut f = fs::File::create(p).unwrap();
            f.write_all(contents.as_bytes()).unwrap();
        }
    }

    fn tmpdir(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "dslp-base-disc-{}-{}",
            label,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn finds_matching_extensions() {
        let dir = tmpdir("matching");
        make_tree(
            &dir,
            &[
                ("src/a.rs", "fn a() {}"),
                ("src/b.rs", "fn b() {}"),
                ("src/c.txt", "not rust"),
            ],
        );
        let mut found = discover_files(&dir, &["rs"], &[]);
        found.sort();
        let expect: Vec<PathBuf> = vec![dir.join("src/a.rs"), dir.join("src/b.rs")];
        assert_eq!(found, expect);
    }

    #[test]
    fn respects_gitignore() {
        let dir = tmpdir("gitignore");
        make_tree(
            &dir,
            &[
                (".gitignore", "ignored.rs\n"),
                ("ignored.rs", "fn ignored() {}"),
                ("kept.rs", "fn kept() {}"),
            ],
        );
        let found = discover_files(&dir, &["rs"], &[]);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("kept.rs"));
    }

    #[test]
    fn excludes_match_whole_path_components() {
        let dir = tmpdir("component-excludes");
        make_tree(
            &dir,
            &[("src/a.rs", "fn a() {}"), ("src2/b.rs", "fn b() {}")],
        );
        let found = discover_files(&dir, &["rs"], &["src".to_string()]);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("src2/b.rs"));
    }

    #[test]
    fn user_excludes_apply() {
        let dir = tmpdir("excludes");
        make_tree(
            &dir,
            &[
                ("vendor/x.rs", "fn x() {}"),
                ("src/y.rs", "fn y() {}"),
            ],
        );
        let found = discover_files(&dir, &["rs"], &["vendor/".to_string()]);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("y.rs"));
    }
}
