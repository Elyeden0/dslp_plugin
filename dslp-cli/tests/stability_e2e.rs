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

