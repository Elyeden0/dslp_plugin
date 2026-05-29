//! Core CLI behavior: scan / next / findings / score plus --json paths.

mod common;

use std::process::{Command, Stdio};

use common::{dslp_bin, fresh_dir, rust_fixture, rust_fixture_many_findings};

#[test]
fn scan_then_next_produces_expected_output() {
    let project = rust_fixture("happy");

    let out = Command::new(dslp_bin())
        .args(["scan", "--path"])
        .arg(&project)
        .output()
        .expect("dslp scan should run");
    assert!(out.status.success(), "scan failed: {:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("Score:"), "scan output missing score: {}", stdout);

    let state_path = project.join(".dslp/state.json");
    assert!(state_path.exists(), "expected state.json to be written");

    let out = Command::new(dslp_bin())
        .args(["next", "--path"])
        .arg(&project)
        .output()
        .expect("dslp next should run");
    assert!(out.status.success(), "next failed: {:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("rust."), "next output missing detector id: {}", stdout);
    // The "When done" line tells the agent how to clear the issue.
    assert!(stdout.contains("dslp resolve"), "next output missing resolve hint: {}", stdout);
}

#[test]
fn findings_lists_all_unresolved() {
    let project = rust_fixture("findings");

    let _ = Command::new(dslp_bin())
        .args(["scan", "--path"])
        .arg(&project)
        .output()
        .unwrap();

    let out = Command::new(dslp_bin())
        .args(["findings", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(out.status.success(), "findings failed: {:?}", out);
    let stdout = String::from_utf8(out.stdout).unwrap();
    let issue_lines = stdout.lines().filter(|l| l.starts_with("[")).count();
    assert!(
        issue_lines >= 2,
        "expected multiple issues listed, got: {}",
        stdout
    );

    let out = Command::new(dslp_bin())
        .args(["findings", "--json", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("findings json parses");
    assert!(v.is_array(), "expected array, got: {}", v);
    assert!(!v.as_array().unwrap().is_empty(), "expected non-empty array");
}

#[test]
fn json_outputs_are_parseable() {
    let project = rust_fixture("json");

    let out = Command::new(dslp_bin())
        .args(["scan", "--json", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("scan json parses");
    assert!(v["score"]["lenient"].is_number(), "got: {}", v);
    assert!(v["score"]["strict"].is_number(), "got: {}", v);
    assert!(v["files_scanned"].is_number(), "got: {}", v);

    let out = Command::new(dslp_bin())
        .args(["next", "--json", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(out.status.success(), "next failed: {:?}", out);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("next json parses");
    assert!(v["fingerprint"].is_string(), "expected fingerprint, got: {}", v);
    assert!(v["detector"].is_string(), "expected detector, got: {}", v);

    let out = Command::new(dslp_bin())
        .args(["score", "--json", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("score json parses");
    assert!(v["lenient"].is_number(), "got: {}", v);
    assert!(v["strict"].is_number(), "got: {}", v);
}

#[test]
fn findings_survives_reader_closing_pipe_early() {
    // Regression: `dslp findings --json | head -1` used to panic with
    // "failed printing to stdout: Broken pipe". Resetting SIGPIPE to its
    // default disposition makes the process die by signal instead of
    // unwinding, so nothing is printed to stderr.
    let project = rust_fixture_many_findings("pipe_close", 500);

    let scan = Command::new(dslp_bin())
        .args(["scan", "--path"])
        .arg(&project)
        .output()
        .expect("dslp scan should run");
    assert!(scan.status.success(), "scan failed: {:?}", scan);

    let mut child = Command::new(dslp_bin())
        .args(["findings", "--json", "--path"])
        .arg(&project)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("dslp findings should spawn");

    // Mimic `| head -1`: close the read end without draining stdout. The JSON
    // output is hundreds of KB — far past the pipe buffer — so the writer is
    // still mid-stream when the reader goes away.
    drop(child.stdout.take());

    let out = child.wait_with_output().expect("child should exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("panic") && !stderr.contains("Broken pipe"),
        "findings panicked on a closed pipe; stderr was:\n{stderr}"
    );
}

#[test]
fn next_without_scan_exits_one() {
    let project = fresh_dir("no_scan");
    let out = Command::new(dslp_bin())
        .args(["next", "--path"])
        .arg(&project)
        .output()
        .expect("dslp next should run");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("No scan"), "expected 'No scan' message, got: {}", stderr);
}
