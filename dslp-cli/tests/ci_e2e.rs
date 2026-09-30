//! Post-scan summary and CI gating flags.

mod common;

use std::process::{Command, Output};

use common::{dslp_bin, fresh_dir};

fn run(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(dslp_bin()).args(args).current_dir(dir).output().unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn complex_rs(name: &str) -> String {
    let body: String = (0..12).map(|i| format!("    if x == {i} {{ return {i}; }}\n")).collect();
    format!("fn {name}(x: i32) -> i32 {{\n{body}    0\n}}\nfn main() {{ {name}(1); }}\n")
}

#[test]
fn scan_reports_change_since_last_scan() {
    let dir = fresh_dir("ci-delta");
    std::fs::write(dir.join("a.rs"), complex_rs("helper")).unwrap();
    let first = run(&["scan"], &dir);
    assert!(!stdout(&first).contains("Since last scan"));

    std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
    let second = run(&["scan"], &dir);
    assert!(stdout(&second).contains("+0 new, 1 fixed, 0 regressed"), "{}", stdout(&second));

    let json: serde_json::Value = serde_json::from_slice(&run(&["scan", "--json"], &dir).stdout).unwrap();
    assert_eq!(json["delta"]["new"], 0);
}

#[test]
fn fail_under_gates_on_strict_score() {
    let dir = fresh_dir("ci-under");
    std::fs::write(dir.join("a.rs"), complex_rs("helper")).unwrap();
    assert_eq!(run(&["scan", "--fail-under", "99.9"], &dir).status.code(), Some(1));
    assert_eq!(run(&["scan", "--fail-under", "50"], &dir).status.code(), Some(0));
}

#[test]
fn fail_on_new_only_fires_with_a_baseline() {
    let dir = fresh_dir("ci-new");
    std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
    assert_eq!(run(&["scan", "--fail-on-new"], &dir).status.code(), Some(0));

    std::fs::write(dir.join("b.rs"), complex_rs("helper")).unwrap();
    let out = run(&["scan", "--fail-on-new"], &dir);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("1 new"));
    // Same code again: nothing new, so it passes.
    assert_eq!(run(&["scan", "--fail-on-new"], &dir).status.code(), Some(0));

    let fresh = fresh_dir("ci-new-first");
    std::fs::write(fresh.join("b.rs"), complex_rs("helper")).unwrap();
    assert_eq!(run(&["scan", "--fail-on-new"], &fresh).status.code(), Some(0), "first scan has no baseline");
}
