//! Habits common in AI-written Rust: leftover `dbg!` and unfinished `todo!`.

use std::path::Path;

use dslp_base::SlopConfig;
use dslp_core::{Finding, Position, Severity, Span};
use dslp_lang_common::{item_finding, walk};
use tree_sitter::{Node, Tree};

const FN_KINDS: &[&str] = &["function_item"];

pub fn run(path: &Path, source: &str, tree: &Tree, cfg: &SlopConfig) -> Vec<Finding> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    walk(tree.root_node(), &mut |node| {
        if node.kind() != "macro_invocation" {
            return;
        }
        let name = node
            .child_by_field_name("macro")
            .and_then(|m| m.utf8_text(bytes).ok());
        match name {
            Some("dbg") => out.push(item_finding(
                path,
                node,
                bytes,
                FN_KINDS,
                "rust.leftover-debug",
                Severity::Low,
                "`dbg!` left in code",
                "Remove it before committing.",
            )),
            Some(m @ ("todo" | "unimplemented")) => out.push(item_finding(
                path,
                node,
                bytes,
                FN_KINDS,
                "rust.unfinished",
                Severity::Medium,
                &format!("`{m}!` will panic at runtime"),
                "Implement it, or return a proper error if the case is genuinely unsupported.",
            )),
            _ => {}
        }
    });
    out.extend(unwrap_density(path, bytes, tree, cfg));
    out
}

/// One file-level finding when non-test code leans on `unwrap`/`expect`.
fn unwrap_density(path: &Path, bytes: &[u8], tree: &Tree, cfg: &SlopConfig) -> Option<Finding> {
    let in_tests_dir = path.components().any(|c| c.as_os_str() == "tests");
    if in_tests_dir {
        return None;
    }
    let mut count = 0u32;
    let mut first: Option<Node> = None;
    count_unwraps(tree.root_node(), bytes, &mut count, &mut first);
    let at = first.filter(|_| count >= cfg.unwrap_threshold)?;
    let line = at.start_position().row as u32 + 1;
    Some(Finding::new(
        "rust.unwrap-density",
        Severity::Low,
        path.to_path_buf(),
        Some(Span::new(Position::new(line, 0), Position::new(line, 0))),
        format!(
            "{count} `unwrap()`/`expect()` calls outside tests (threshold {}). Each is a potential panic.",
            cfg.unwrap_threshold
        ),
        Some("Propagate errors with `?` and a typed error, or handle the None/Err case explicitly.".into()),
        "unwrap-density",
    ))
}

fn count_unwraps<'a>(node: Node<'a>, bytes: &[u8], count: &mut u32, first: &mut Option<Node<'a>>) {
    if is_test_scope(node, bytes) {
        return;
    }
    if node.kind() == "call_expression" && is_unwrap_call(node, bytes) {
        *count += 1;
        first.get_or_insert(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        count_unwraps(child, bytes, count, first);
    }
}

fn is_unwrap_call(call: Node, bytes: &[u8]) -> bool {
    let method = call
        .child_by_field_name("function")
        .filter(|f| f.kind() == "field_expression")
        .and_then(|f| f.child_by_field_name("field"))
        .and_then(|f| f.utf8_text(bytes).ok());
    matches!(method, Some("unwrap" | "expect"))
}

/// `mod tests { .. }` or a `#[test]` / `#[cfg(test)]` item.
fn is_test_scope(node: Node, bytes: &[u8]) -> bool {
    if !matches!(node.kind(), "mod_item" | "function_item") {
        return false;
    }
    let mut prev = node.prev_sibling();
    while let Some(sib) = prev.filter(|s| s.kind() == "attribute_item") {
        let text = sib.utf8_text(bytes).unwrap_or_default();
        if text.contains("test") {
            return true;
        }
        prev = sib.prev_sibling();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detectors(src: &str) -> Vec<String> {
        let tree = dslp_base::parse("rust", src).unwrap();
        run(Path::new("a.rs"), src, &tree, &SlopConfig::default())
            .into_iter()
            .map(|f| f.detector)
            .collect()
    }

    #[test]
    fn dbg_and_todo_fire_but_println_does_not() {
        let got = detectors("fn f() { dbg!(1); todo!(); unimplemented!(); println!(\"x\"); }");
        assert_eq!(
            got,
            ["rust.leftover-debug", "rust.unfinished", "rust.unfinished"]
        );
    }

    fn density(src: &str, path: &str) -> Vec<String> {
        let tree = dslp_base::parse("rust", src).unwrap();
        let cfg = SlopConfig {
            unwrap_threshold: 3,
            ..SlopConfig::default()
        };
        run(Path::new(path), src, &tree, &cfg)
            .into_iter()
            .map(|f| f.detector)
            .collect()
    }

    #[test]
    fn unwrap_density_counts_unwrap_and_expect_outside_tests() {
        let many = "fn f() { a().unwrap(); b().expect(\"x\"); c().unwrap(); }";
        assert_eq!(density(many, "src/a.rs"), ["rust.unwrap-density"]);
        let few = "fn f() { a().unwrap(); b().expect(\"x\"); }";
        assert!(density(few, "src/a.rs").is_empty());
        assert!(density(
            "fn f() { a().unwrap_or(1); b().unwrap_or_default(); c().unwrap_or(2); }",
            "src/a.rs"
        )
        .is_empty());
    }

    #[test]
    fn unwrap_density_ignores_test_code() {
        let in_mod = "#[cfg(test)]\nmod tests {\n    fn t() { a().unwrap(); b().unwrap(); c().unwrap(); }\n}\n";
        assert!(density(in_mod, "src/a.rs").is_empty());
        let test_fn = "#[test]\nfn t() { a().unwrap(); b().unwrap(); c().unwrap(); }\n";
        assert!(density(test_fn, "src/a.rs").is_empty());
        let many = "fn f() { a().unwrap(); b().unwrap(); c().unwrap(); }";
        assert!(
            density(many, "tests/it.rs").is_empty(),
            "integration tests are exempt"
        );
    }
}
