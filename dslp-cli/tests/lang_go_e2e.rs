//! E2E test for the Go plugin.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

#[test]
fn go_project_complexity_detected() {
    let dir = fresh_dir("go");
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

    let _ = Command::new(dslp_bin())
        .args(["scan", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    let out = Command::new(dslp_bin())
        .args(["findings", "--json", "--path"])
        .arg(&dir)
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let arr = v.as_array().unwrap();
    assert!(
        arr.iter().any(|i| i["detector"] == "go.complexity"),
        "expected go.complexity, got: {}",
        v
    );
}
