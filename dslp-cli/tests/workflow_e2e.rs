//! E2E for the workflow loop: scan → suppress → score gap → rescan preserves
//! wontfix; resolve advances `next`.

mod common;

use std::process::Command;

use common::{dslp_bin, rust_fixture};

/// Pull the first fingerprint out of `findings --json`.
fn first_fingerprint(project: &std::path::Path) -> String {
    let out = Command::new(dslp_bin())
        .args(["findings", "--json", "--path"])
        .arg(project)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    v.as_array().unwrap()[0]["fingerprint"]
        .as_str()
        .unwrap()
        .to_string()
}

fn scan(project: &std::path::Path) {
    let out = Command::new(dslp_bin())
        .args(["scan", "--path"])
        .arg(project)
        .output()
        .unwrap();
    assert!(out.status.success(), "scan failed: {:?}", out);
}

#[test]
fn resolve_advances_next() {
    let project = rust_fixture("wf-resolve");
    scan(&project);
    let fp = first_fingerprint(&project);

    let out = Command::new(dslp_bin())
        .args(["resolve"])
        .arg(&fp)
        .args(["--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(out.status.success(), "resolve failed: {:?}", out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("Resolved"));

    // The resolved fingerprint must no longer be the head of the queue.
    let out = Command::new(dslp_bin())
        .args(["findings", "--json", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let still_open: Vec<&str> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["fingerprint"].as_str().unwrap())
        .collect();
    assert!(!still_open.contains(&fp.as_str()), "resolved issue should leave the open queue");
}

/// Scan `project` and suppress its first open issue with a reason.
fn suppress_first(project: &std::path::Path) {
    scan(project);
    let fp = first_fingerprint(project);
    let out = Command::new(dslp_bin())
        .args(["suppress"])
        .arg(&fp)
        .args(["--reason", "accepted: generated fixture file"])
        .args(["--path"])
        .arg(project)
        .output()
        .unwrap();
    assert!(out.status.success(), "suppress failed: {:?}", out);
}

fn score_json(project: &std::path::Path) -> serde_json::Value {
    let out = Command::new(dslp_bin())
        .args(["score", "--json", "--path"])
        .arg(project)
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn suppress_requires_reason() {
    let project = rust_fixture("wf-noreason");
    scan(&project);
    let fp = first_fingerprint(&project);
    let no_reason = Command::new(dslp_bin())
        .args(["suppress"])
        .arg(&fp)
        .args(["--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(!no_reason.status.success(), "suppress without --reason should fail");
}

#[test]
fn suppress_widens_gap() {
    let project = rust_fixture("wf-suppress");
    suppress_first(&project);
    let s = score_json(&project);
    let lenient = s["lenient"].as_f64().unwrap();
    let strict = s["strict"].as_f64().unwrap();
    assert!(lenient >= strict, "lenient must be ≥ strict");
    assert!(lenient - strict > 0.0, "suppressing should open a gap");
    assert!(s["wontfix_count"].as_u64().unwrap() >= 1);
}

#[test]
fn wontfix_survives_rescan() {
    let project = rust_fixture("wf-rescan");
    suppress_first(&project);
    scan(&project); // rescan: the suppressed issue is still detected
    let s = score_json(&project);
    assert!(
        s["wontfix_count"].as_u64().unwrap() >= 1,
        "wontfix should survive a rescan, got {}",
        s
    );
}

#[test]
fn unknown_fingerprint_errors() {
    let project = rust_fixture("wf-unknown");
    scan(&project);
    let out = Command::new(dslp_bin())
        .args(["resolve", "zzzzzzzz", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(!out.status.success(), "unknown fingerprint should error");
    assert!(String::from_utf8(out.stderr).unwrap().contains("no issue matches"));
}
