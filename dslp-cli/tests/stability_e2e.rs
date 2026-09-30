//! Issue identity must survive edits and path spelling; JS/TS LoC is counted once.

mod common;

use std::process::{Command, Output};

use common::{dslp_bin, fresh_dir};

fn run(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(dslp_bin()).args(args).current_dir(dir).output().unwrap()
}

fn complex_rs() -> String {
    let body: String = (0..12).map(|i| format!("    if x == {i} {{ return {i}; }}\n")).collect();
    format!("fn helper(x: i32) -> i32 {{\n{body}    0\n}}\nfn main() {{ helper(1); }}\n")
}

fn json(out: &Output) -> serde_json::Value {
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn wontfix_survives_line_shift_and_metric_drift() {
    let dir = fresh_dir("stab-shift");
    std::fs::write(dir.join("a.rs"), complex_rs()).unwrap();
    run(&["scan"], &dir);
    let fp = json(&run(&["findings", "--json"], &dir))[0]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(run(&["suppress", &fp, "--reason", "ok"], &dir).status.success());

    std::fs::write(dir.join("a.rs"), format!("// header\n{}", complex_rs().replace("0\n}", "1 + 1\n}"))).unwrap();
    run(&["scan"], &dir);
    let backlog = json(&run(&["backlog", "--json"], &dir));
    assert_eq!(backlog.as_array().unwrap().len(), 1, "suppression was lost");
    assert!(json(&run(&["findings", "--json"], &dir)).as_array().unwrap().is_empty());
}

#[test]
fn path_spelling_does_not_split_history() {
    let dir = fresh_dir("stab-path");
    std::fs::write(dir.join("a.rs"), complex_rs()).unwrap();
    run(&["scan", "."], &dir);
    run(&["scan", dir.to_str().unwrap()], &dir);
    let issues = json(&run(&["findings", "--json"], &dir));
    assert_eq!(issues.as_array().unwrap().len(), 1);
    assert_eq!(issues[0]["file"], "a.rs");
}

#[test]
fn shared_extensions_count_loc_once() {
    let dir = fresh_dir("stab-loc");
    std::fs::write(dir.join("app.ts"), "export const x = 1;\n".repeat(10)).unwrap();
    std::fs::write(dir.join("package.json"), r#"{"dependencies":{"react":"1","next":"1","nuxt":"1"}}"#).unwrap();
    let v = json(&run(&["scan", "--json"], &dir));
    assert_eq!(v["total_loc"], 10);
    assert_eq!(v["files_scanned"], 1);
}

#[test]
fn unknown_lang_is_rejected_by_every_command() {
    let dir = fresh_dir("stab-lang");
    for cmd in ["next", "score", "findings", "plan", "backlog", "badge"] {
        let out = run(&[cmd, "--lang", "klingon"], &dir);
        assert_eq!(out.status.code(), Some(2), "{cmd}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("unknown --lang"), "{cmd}");
    }
}

#[test]
fn reopen_undoes_suppress() {
    let dir = fresh_dir("stab-reopen");
    std::fs::write(dir.join("a.rs"), complex_rs()).unwrap();
    run(&["scan"], &dir);
    let fp = json(&run(&["findings", "--json"], &dir))[0]["fingerprint"].as_str().unwrap().to_string();
    run(&["suppress", &fp, "--reason", "ok"], &dir);
    assert!(run(&["reopen", &fp], &dir).status.success());
    assert_eq!(json(&run(&["findings", "--json"], &dir)).as_array().unwrap().len(), 1);
}

#[test]
fn positional_path_and_flag_conflict() {
    let dir = fresh_dir("stab-conflict");
    let out = run(&["scan", ".", "--path", "."], &dir);
    assert!(!out.status.success());
}
