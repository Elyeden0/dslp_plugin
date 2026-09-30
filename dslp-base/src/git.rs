use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context};

fn git_lines(root: &Path, args: &[&str]) -> anyhow::Result<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("failed to run `git` (is it installed?)")?;
    if !out.status.success() {
        bail!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

/// Files under `root` that differ from `rev` in the working tree (added,
/// modified, deleted or renamed), plus untracked files, as paths relative to
/// `root`.
pub fn changed_files(root: &Path, rev: &str) -> anyhow::Result<HashSet<PathBuf>> {
    let mut changed: Vec<String> = git_lines(root, &["diff", "--name-only", "--relative", rev])?;
    changed.extend(git_lines(
        root,
        &["ls-files", "--others", "--exclude-standard"],
    )?);
    Ok(changed.into_iter().map(PathBuf::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    #[test]
    fn reports_modified_deleted_and_untracked_files() {
        let dir = std::env::temp_dir().join(format!("dslp-base-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        for f in ["keep.rs", "edit.rs", "gone.rs"] {
            std::fs::write(dir.join(f), "fn a() {}\n").unwrap();
        }
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-qm", "init"]);
        std::fs::write(dir.join("edit.rs"), "fn b() {}\n").unwrap();
        std::fs::remove_file(dir.join("gone.rs")).unwrap();
        std::fs::write(dir.join("new.rs"), "fn c() {}\n").unwrap();

        let mut got: Vec<_> = changed_files(&dir, "HEAD").unwrap().into_iter().collect();
        got.sort();
        assert_eq!(got, ["edit.rs", "gone.rs", "new.rs"].map(PathBuf::from));
    }

    #[test]
    fn bad_revision_is_an_error() {
        let dir = std::env::temp_dir().join(format!("dslp-base-git-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]);
        assert!(changed_files(&dir, "no-such-rev").is_err());
    }
}
