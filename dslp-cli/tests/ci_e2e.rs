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

#[test]
fn duplicate_blocks_are_reported_across_languages_and_can_be_disabled() {
    let dir = fresh_dir("dup-e2e");
    let shared: String = (0..10)
        .map(|i| format!("    total += weigh(item_{i});\n"))
        .collect();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.rs"), format!("fn a() {{\n{shared}}}\n")).unwrap();
    std::fs::write(dir.join("src/b.rs"), format!("fn b() {{\n{shared}}}\n")).unwrap();
    run(&["scan"], &dir);
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    let dups: Vec<_> = v
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["detector"] == "dup.block")
        .collect();
    assert_eq!(dups.len(), 1, "{v}");
    assert_eq!(dups[0]["file"], "src/b.rs");

    std::fs::create_dir_all(dir.join(".dslp")).unwrap();
    std::fs::write(
        dir.join(".dslp/config.toml"),
        "[detectors]\ndisable = [\"dup.*\"]\n",
    )
    .unwrap();
    run(&["scan"], &dir);
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    assert!(v.as_array().unwrap().is_empty());
}

#[test]
fn comment_slop_is_detected_in_rust_and_python() {
    let dir = fresh_dir("comment-slop");
    std::fs::write(
        dir.join("a.rs"),
        "fn main() {\n    // let old = compute(1);\n    // if old > 0 {\n    //     return old;\n    // }\n    // return the result\n    return_it(result);\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("b.py"),
        "# x = load(1)\n# y = load(2)\n# print(x, y)\nz = 1\n",
    )
    .unwrap();
    run(&["scan"], &dir);
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], &dir).stdout).unwrap();
    let ids: Vec<&str> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["detector"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"rust.commented-code"), "{ids:?}");
    assert!(ids.contains(&"python.commented-code"), "{ids:?}");
}

#[test]
fn ts_dead_exports_are_flagged_except_in_libraries_and_convention_files() {
    let dir = fresh_dir("ts-dead-export");
    std::fs::create_dir_all(dir.join("src/lib")).unwrap();
    std::fs::write(
        dir.join("src/lib/util.ts"),
        "export function used() {}\nexport function orphan() {}\nexport interface Shape {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/app.ts"),
        "import { used } from './lib/util';\nused();\n",
    )
    .unwrap();
    std::fs::write(dir.join("src/index.ts"), "export const entry = 1;\n").unwrap();
    let dead = |dir: &std::path::Path| -> Vec<String> {
        run(&["scan"], dir);
        let v: serde_json::Value =
            serde_json::from_slice(&run(&["findings", "--json"], dir).stdout).unwrap();
        v.as_array()
            .unwrap()
            .iter()
            .filter(|i| i["detector"] == "ts.dead-export")
            .map(|i| i["message"].as_str().unwrap().to_string())
            .collect()
    };
    let found = dead(&dir);
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found.iter().any(|m| m.contains("`orphan`")) && found.iter().any(|m| m.contains("`Shape`"))
    );

    std::fs::write(
        dir.join("package.json"),
        r#"{"name":"lib","main":"dist/index.js"}"#,
    )
    .unwrap();
    assert!(
        dead(&dir).is_empty(),
        "library packages export their API on purpose"
    );
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@t"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Same shape as `complex_rs` but with different lines, so the duplicate
/// detector doesn't (correctly) flag the fixtures against each other.
fn complex_rs_variant(name: &str, salt: u32) -> String {
    complex_rs(name).replace("return ", &format!("return {salt} + "))
}

fn open_files(dir: &std::path::Path) -> Vec<String> {
    let v: serde_json::Value =
        serde_json::from_slice(&run(&["findings", "--json"], dir).stdout).unwrap();
    let mut files: Vec<String> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["file"].as_str().unwrap().to_string())
        .collect();
    files.sort();
    files
}

#[test]
fn since_reports_only_changed_files_and_keeps_the_rest_untouched() {
    let dir = fresh_dir("since");
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join("a.rs"), complex_rs("alpha")).unwrap();
    std::fs::write(dir.join("b.rs"), complex_rs_variant("beta", 1000)).unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "init"]);
    run(&["scan"], &dir);
    assert_eq!(open_files(&dir), ["a.rs", "b.rs"]);

    // b.rs is fixed and c.rs is new; a.rs is untouched.
    std::fs::write(dir.join("b.rs"), "fn main() {}\n").unwrap();
    std::fs::write(dir.join("c.rs"), complex_rs_variant("gamma", 2000)).unwrap();
    let out = run(&["scan", "--since", "HEAD"], &dir);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout(&out).contains("changed since HEAD"),
        "{}",
        stdout(&out)
    );
    assert_eq!(
        open_files(&dir),
        ["a.rs", "c.rs"],
        "a.rs kept, b.rs resolved, c.rs new"
    );

    let bad = run(&["scan", "--since", "no-such-rev"], &dir);
    assert_eq!(bad.status.code(), Some(2));
}

#[test]
fn since_does_not_resolve_issues_in_files_it_did_not_report_on() {
    let dir = fresh_dir("since-keep");
    git(&dir, &["init", "-q"]);
    std::fs::write(dir.join("a.rs"), complex_rs("alpha")).unwrap();
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-qm", "init"]);
    run(&["scan"], &dir);
    // Nothing changed since HEAD: a partial scan must leave a.rs's issue open.
    run(&["scan", "--since", "HEAD"], &dir);
    assert_eq!(open_files(&dir), ["a.rs"]);
}
