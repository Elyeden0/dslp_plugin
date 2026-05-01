//! E2E test for the C++ plugin.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

#[test]
fn cpp_project_complexity_detected() {
    let dir = fresh_dir("cpp");
    let src = r#"
#include <vector>

int tangle(int x) {
    if (x > 0) {
        if (x < 10) return x;
        if (x < 20) return x * 2;
        if (x < 30) return x * 3;
        if (x < 40) return x * 4;
        if (x < 50) return x * 5;
    }
    while (x > 100) {
        if (x % 2 == 0) return x;
        if (x % 3 == 0) return x;
        if (x % 5 == 0) return x;
    }
    for (int i = 0; i < 100; i++) {
        if (i > 50) return i;
    }
    return x;
}
"#;
    std::fs::write(dir.join("main.cpp"), src).unwrap();

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
        arr.iter().any(|i| i["detector"] == "cpp.complexity"),
        "expected cpp.complexity, got: {}",
        v
    );
}
