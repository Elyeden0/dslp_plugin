//! E2E test for the Python plugin.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

#[test]
fn python_project_complexity_detected() {
    let dir = fresh_dir("python");
    let src = r#"
def tangle(x):
    if x > 0:
        if x < 10:
            return x
        elif x < 20:
            return x * 2
        elif x < 30:
            return x * 3
        elif x < 40:
            return x * 4
        elif x < 50:
            return x * 5
    while x > 100:
        if x % 2 == 0:
            return x
        if x % 3 == 0:
            return x
        if x % 5 == 0:
            return x
    for i in range(100):
        if i > 50:
            return i
    return x
"#;
    std::fs::write(dir.join("main.py"), src).unwrap();

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
        arr.iter().any(|i| i["detector"] == "python.complexity"),
        "expected python.complexity, got: {}",
        v
    );
}
