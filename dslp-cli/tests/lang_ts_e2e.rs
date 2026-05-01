//! E2E test for the TS/JS plugin.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

#[test]
fn typescript_project_is_scanned() {
    let dir = fresh_dir("ts");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let src = r#"
function tangle(x: number): number {
    if (x > 0) {
        if (x < 10) return x;
        if (x < 20) return x * 2;
        if (x < 30) return x * 3;
        if (x < 40) return x * 4;
        if (x < 50) return x * 5;
    }
    while (x > 100) {
        if (x % 2 === 0) return x;
        if (x % 3 === 0) return x;
        if (x % 5 === 0) return x;
    }
    for (let i = 0; i < 10; i++) {
        if (i > 5) return i;
    }
    return x;
}
"#;
    std::fs::write(dir.join("src/app.ts"), src).unwrap();

    let out = Command::new(dslp_bin())
        .args(["scan", "--path"])
        .arg(&dir)
        .output()
        .expect("dslp scan should run");
    assert!(out.status.success(), "scan failed: {:?}", out);

    let out = Command::new(dslp_bin())
        .args(["next", "--path"])
        .arg(&dir)
        .output()
        .expect("dslp next should run");
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(
        stdout.contains("ts.complexity") || stdout.contains("ts."),
        "expected a ts.* finding, got: {}",
        stdout
    );
}
