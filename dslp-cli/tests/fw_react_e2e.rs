//! E2E tests for the React framework plugin.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

#[test]
fn react_hook_bloat_detected_in_tsx_project() {
    let dir = fresh_dir("react");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"dependencies":{"react":"^18.0.0"}}"#,
    )
    .unwrap();

    let body: String = (0..15)
        .map(|i| format!("        field{}: {}", i, i))
        .collect::<Vec<_>>()
        .join(",\n");
    let hook = format!(
        "export function useFat() {{\n    return {{\n{}\n    }};\n}}\n",
        body
    );
    std::fs::write(dir.join("src/useFat.tsx"), hook).unwrap();

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
    let arr = v.as_array().expect("findings is array");
    assert!(
        arr.iter()
            .any(|i| i["detector"] == "react.hook-return-bloat"),
        "expected react.hook-return-bloat among findings: {}",
        v
    );
}

#[test]
fn react_plugin_silent_in_non_react_project() {
    let dir = fresh_dir("no_react");
    std::fs::create_dir_all(dir.join("src")).unwrap();

    let body: String = (0..15)
        .map(|i| format!("        field{}: {}", i, i))
        .collect::<Vec<_>>()
        .join(",\n");
    let hook = format!(
        "export function useFat() {{\n    return {{\n{}\n    }};\n}}\n",
        body
    );
    std::fs::write(dir.join("src/useFat.ts"), hook).unwrap();

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
        !arr.iter()
            .any(|i| i["detector"] == "react.hook-return-bloat"),
        "react detector should not fire on non-React project: {}",
        v
    );
}
