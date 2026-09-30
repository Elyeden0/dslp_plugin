//! Post-scan summary and CI gating flags.

mod common;

use std::process::{Command, Output};

use common::{dslp_bin, fresh_dir};

fn run(args: &[&str], dir: &std::path::Path) -> Output {
    Command::new(dslp_bin())
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn complex_rs(name: &str) -> String {
    let body: String = (0..12)
        .map(|i| format!("    if x == {i} {{ return {i}; }}\n"))
        .collect();
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
    assert!(
        stdout(&second).contains("+0 new, 1 fixed, 0 regressed"),
        "{}",
        stdout(&second)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&run(&["scan", "--json"], &dir).stdout).unwrap();
    assert_eq!(json["delta"]["new"], 0);
}

#[test]
fn fail_under_gates_on_strict_score() {
    let dir = fresh_dir("ci-under");
    std::fs::write(dir.join("a.rs"), complex_rs("helper")).unwrap();
    assert_eq!(
        run(&["scan", "--fail-under", "99.9"], &dir).status.code(),
        Some(1)
    );
    assert_eq!(
        run(&["scan", "--fail-under", "50"], &dir).status.code(),
        Some(0)
    );
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
    assert_eq!(
        run(&["scan", "--fail-on-new"], &fresh).status.code(),
        Some(0),
        "first scan has no baseline"
    );
}

#[test]
fn init_writes_config_and_gitignore_once() {
    let dir = fresh_dir("wf-init");
    std::fs::write(dir.join(".gitignore"), "target").unwrap();
    assert!(run(&["init"], &dir).status.success());
    assert!(dir.join(".dslp/config.toml").exists());
    assert_eq!(
        std::fs::read_to_string(dir.join(".gitignore")).unwrap(),
        "target\n.dslp/\n"
    );
    // The generated config must load, and init must not clobber it.
    std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
    assert!(run(&["scan"], &dir).status.success());
    assert_eq!(run(&["init"], &dir).status.code(), Some(2));
}

#[test]
fn show_displays_wontfix_reason() {
    let dir = fresh_dir("wf-show");
    std::fs::write(dir.join("a.rs"), complex_rs("helper")).unwrap();
    run(&["scan"], &dir);
    let listed: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    let fp = listed[0]["fingerprint"].as_str().unwrap().to_string();
    run(&["suppress", &fp, "--reason", "legacy parser"], &dir);
    let out = stdout(&run(&["show", &fp[..8]], &dir));
    assert!(
        out.contains("wontfix") && out.contains("legacy parser"),
        "{out}"
    );
}

#[test]
fn config_can_disable_detectors_and_override_severity() {
    let dir = fresh_dir("cfg-overrides");
    std::fs::write(dir.join("a.rs"), complex_rs("helper")).unwrap();
    std::fs::create_dir_all(dir.join(".dslp")).unwrap();
    std::fs::write(
        dir.join(".dslp/config.toml"),
        "[detectors.severity]\n\"rust.complexity\" = \"critical\"\n",
    )
    .unwrap();
    run(&["scan"], &dir);
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    assert_eq!(v[0]["severity"], "critical");

    std::fs::write(
        dir.join(".dslp/config.toml"),
        "[detectors]\ndisable = [\"rust.*\"]\n",
    )
    .unwrap();
    run(&["scan"], &dir);
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    assert!(v.as_array().unwrap().is_empty());
}

#[test]
fn inline_ignore_becomes_wontfix_and_still_counts_for_strict() {
    let dir = fresh_dir("wf-inline");
    let src = complex_rs("helper");
    std::fs::write(dir.join("a.rs"), &src).unwrap();
    run(&["scan"], &dir);
    let open_score: serde_json::Value =
        serde_json::from_slice(&run(&["score", "--json"], &dir).stdout).unwrap();

    std::fs::write(
        dir.join("a.rs"),
        format!("// dslp-ignore rust.complexity: generated tables\n{src}"),
    )
    .unwrap();
    run(&["scan"], &dir);
    let backlog: serde_json::Value =
        serde_json::from_slice(&run(&["backlog", "--json"], &dir).stdout).unwrap();
    assert_eq!(backlog.as_array().unwrap().len(), 1);
    assert_eq!(backlog[0]["status"]["reason"], "inline: generated tables");
    let score: serde_json::Value =
        serde_json::from_slice(&run(&["score", "--json"], &dir).stdout).unwrap();
    assert_eq!(
        score["strict"], open_score["strict"],
        "an inline ignore must not raise the strict score"
    );
    assert!(score["lenient"].as_f64() > open_score["lenient"].as_f64());
}

#[test]
fn per_language_thresholds_override_globals_and_typos_are_rejected() {
    let dir = fresh_dir("cfg-lang");
    std::fs::write(dir.join("a.rs"), complex_rs("helper")).unwrap();
    std::fs::create_dir_all(dir.join(".dslp")).unwrap();
    let cfg = |body: &str| std::fs::write(dir.join(".dslp/config.toml"), body).unwrap();

    cfg("[detectors.languages.rust]\ncyclomatic_threshold = 99\n");
    run(&["scan"], &dir);
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    assert!(
        v.as_array().unwrap().is_empty(),
        "rust override should silence complexity: {v}"
    );

    cfg("[detectors.languages.python]\ncyclomatic_threshold = 99\n");
    run(&["scan"], &dir);
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    assert_eq!(
        v.as_array().unwrap().len(),
        1,
        "python override must not affect rust"
    );

    cfg("[detectors.languages.rsut]\ncyclomatic_threshold = 99\n");
    let out = run(&["scan"], &dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("rsut"));
}

#[test]
fn findings_can_be_emitted_as_sarif() {
    let dir = fresh_dir("ci-sarif");
    std::fs::write(dir.join("a.rs"), complex_rs("helper")).unwrap();
    run(&["scan"], &dir);
    let out = run(&["findings", "--sarif"], &dir);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["version"], "2.1.0");
    assert_eq!(v["runs"][0]["results"][0]["ruleId"], "rust.complexity");
    assert!(
        !run(&["findings", "--sarif", "--json"], &dir)
            .status
            .success(),
        "flags conflict"
    );
}
