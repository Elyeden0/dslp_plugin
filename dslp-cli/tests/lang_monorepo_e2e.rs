//! E2E coverage for `--lang`: scoping a scan to one plugin so a monorepo
//! with more than one language under the same `--path` can be tracked
//! independently per language.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

fn write_go_fixture(dir: &std::path::Path) {
    let src = r#"package main

func tangle(x int) int {
    if x > 0 {
        if x < 10 { return x }
        if x < 20 { return x * 2 }
        if x < 30 { return x * 3 }
        if x < 40 { return x * 4 }
        if x < 50 { return x * 5 }
    }
    for i := 0; i < 100; i++ {
        if i > 50 { return i }
        if i > 60 { return i + 1 }
        if i > 70 { return i + 2 }
        if i > 80 { return i + 3 }
    }
    return x
}

func main() {
    _ = tangle(0)
}
"#;
    std::fs::write(dir.join("main.go"), src).unwrap();
}

fn write_rust_fixture(dir: &std::path::Path) {
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let src = r#"
fn complex(x: i32) -> i32 {
    if x > 0 { if x > 1 { if x > 2 { if x > 3 {
        return x;
    }}}}
    while x > 100 {
        if x % 2 == 0 { return x; }
        if x % 3 == 0 { return x; }
        if x % 5 == 0 { return x; }
        if x % 7 == 0 { return x; }
    }
    for i in 0..100 {
        if i > 50 { return i; }
    }
    x
}

fn caller() {
    let _ = complex(0);
}
"#;
    std::fs::write(dir.join("src/complex.rs"), src).unwrap();
}

#[test]
fn scan_with_lang_only_tracks_that_language() {
    let dir = fresh_dir("monorepo-lang");
    write_go_fixture(&dir);
    write_rust_fixture(&dir);

    let out = Command::new(dslp_bin())
        .args(["scan", "--json", "--lang", "go", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    // A scoped scan writes .dslp/state-go.json, not the shared state.json.
    assert!(dir.join(".dslp/state-go.json").exists());
    assert!(!dir.join(".dslp/state.json").exists());

    let findings = Command::new(dslp_bin())
        .args(["findings", "--json", "--lang", "go", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&findings.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert!(arr.iter().all(|i| i["detector"].as_str().unwrap().starts_with("go.")));
    assert!(!arr.is_empty());
}

#[test]
fn unscoped_and_scoped_scans_are_independent() {
    let dir = fresh_dir("monorepo-independent");
    write_go_fixture(&dir);
    write_rust_fixture(&dir);

    let _ = Command::new(dslp_bin()).args(["scan", "--path"]).arg(&dir).output().unwrap();
    let _ = Command::new(dslp_bin())
        .args(["scan", "--lang", "go", "--path"])
        .arg(&dir)
        .output()
        .unwrap();

    let full = Command::new(dslp_bin())
        .args(["findings", "--json", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    let full_v: serde_json::Value = serde_json::from_slice(&full.stdout).unwrap();
    let full_arr = full_v.as_array().unwrap();
    // The unscoped scan sees both languages.
    assert!(full_arr.iter().any(|i| i["detector"].as_str().unwrap().starts_with("go.")));
    assert!(full_arr.iter().any(|i| i["detector"].as_str().unwrap().starts_with("rust.")));

    let scoped = Command::new(dslp_bin())
        .args(["findings", "--json", "--lang", "go", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    let scoped_v: serde_json::Value = serde_json::from_slice(&scoped.stdout).unwrap();
    let scoped_arr = scoped_v.as_array().unwrap();
    assert!(scoped_arr.iter().all(|i| i["detector"].as_str().unwrap().starts_with("go.")));
}

#[test]
fn unknown_lang_errors() {
    let dir = fresh_dir("monorepo-unknown-lang");
    write_go_fixture(&dir);

    let out = Command::new(dslp_bin())
        .args(["scan", "--lang", "cobol", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown --lang"), "stderr: {stderr}");
}

#[test]
fn resolve_and_suppress_respect_lang_scope() {
    let dir = fresh_dir("monorepo-resolve");
    write_go_fixture(&dir);

    let _ = Command::new(dslp_bin())
        .args(["scan", "--lang", "go", "--path"])
        .arg(&dir)
        .output()
        .unwrap();

    let findings = Command::new(dslp_bin())
        .args(["findings", "--json", "--lang", "go", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&findings.stdout).unwrap();
    let fp = v.as_array().unwrap()[0]["fingerprint"].as_str().unwrap().to_string();

    let resolve = Command::new(dslp_bin())
        .args(["resolve", &fp, "--lang", "go", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(resolve.status.success(), "stderr: {}", String::from_utf8_lossy(&resolve.stderr));

    // Without --lang, the (unscanned) shared state file doesn't exist at all.
    let no_scope = Command::new(dslp_bin()).args(["score", "--json", "--path"]).arg(&dir).output().unwrap();
    assert!(!no_scope.status.success());
}
