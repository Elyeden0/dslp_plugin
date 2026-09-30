//! Shared helpers for E2E CLI tests. Cargo wires `tests/common/mod.rs` so it
//! isn't itself a test target — each per-plugin file declares `mod common;`
//! to reach these helpers.
//!
//! Each integration test file is its own crate, so any helper not used by
//! that specific file appears unused. `#[allow(dead_code)]` quiets the
//! resulting warnings without disabling the lint elsewhere.

#![allow(dead_code)]

use std::path::PathBuf;

pub fn dslp_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dslp"))
}

pub fn fresh_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dslp-cli-e2e-{}-{}", label, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build a fixture whose `dslp findings` output is large (hundreds of KB).
/// Each generated `pub fn` is unreferenced, so the dead-pub detector emits one
/// finding apiece — enough volume that a reader closing the pipe early is
/// guaranteed to leave the writer mid-stream.
pub fn rust_fixture_many_findings(label: &str, count: usize) -> PathBuf {
    let dir = fresh_dir(label);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mut src = String::from("// Auto-generated: unreferenced pub fns.\n");
    for i in 0..count {
        src.push_str(&format!("pub fn dead_{i}() {{}}\n"));
    }
    std::fs::write(dir.join("src/dead.rs"), src).unwrap();
    dir
}

pub fn rust_fixture(label: &str) -> PathBuf {
    let dir = fresh_dir(label);
    std::fs::create_dir_all(dir.join("src")).unwrap();

    let mut long = String::from("// Auto-generated for fixture\n");
    long.push_str("pub fn unused_long_fn() {}\n");
    for i in 0..420 {
        long.push_str(&format!("pub const X{}: u32 = {};\n", i, i));
    }
    std::fs::write(dir.join("src/long.rs"), long).unwrap();

    let complex = r#"
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
    std::fs::write(dir.join("src/complex.rs"), complex).unwrap();

    dir
}
