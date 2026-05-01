//! E2E test for the Next.js framework plugin.

mod common;

use std::process::Command;

use common::{dslp_bin, fresh_dir};

#[test]
fn nextjs_use_client_misplacement_detected() {
    let dir = fresh_dir("nextjs");
    std::fs::create_dir_all(dir.join("app")).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"dependencies":{"next":"^14.0.0","react":"^18.0.0"}}"#,
    )
    .unwrap();
    let src = "import React from 'react';\n\n'use client';\nexport default function Page() { return null; }\n";
    std::fs::write(dir.join("app/page.tsx"), src).unwrap();

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
            .any(|i| i["detector"] == "nextjs.use-client-not-first"),
        "expected nextjs.use-client-not-first, got: {}",
        v
    );
}
