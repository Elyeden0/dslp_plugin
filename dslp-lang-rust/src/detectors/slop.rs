//! Habits common in AI-written Rust: leftover `dbg!` and unfinished `todo!`.

use std::path::Path;

use dslp_core::{Finding, Severity};
use dslp_lang_common::{item_finding, walk};
use tree_sitter::Tree;

const FN_KINDS: &[&str] = &["function_item"];

pub fn run(path: &Path, source: &str, tree: &Tree) -> Vec<Finding> {
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detectors(src: &str) -> Vec<String> {
        let tree = dslp_base::parse("rust", src).unwrap();
        run(Path::new("a.rs"), src, &tree)
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
}
