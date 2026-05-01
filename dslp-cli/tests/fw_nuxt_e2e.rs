//! E2E test for the Nuxt framework plugin.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

#[test]
fn nuxt_misnamed_composable_detected() {
    let dir = fresh_dir("nuxt");
    std::fs::create_dir_all(dir.join("composables")).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"dependencies":{"nuxt":"^3.0.0"}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("composables/fetch.ts"),
        "export function fetchUser() { return null; }\n",
    )
    .unwrap();

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
        arr.iter()
            .any(|i| i["detector"] == "nuxt.composable-naming"),
        "expected nuxt.composable-naming, got: {}",
        v
    );
}
