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
