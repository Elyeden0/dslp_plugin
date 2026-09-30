//! Habits common in AI-written C++: swallowed exceptions.

use std::path::Path;

use dslp_core::{Finding, Severity};
use dslp_lang_common::{item_finding, walk};
use tree_sitter::Tree;

const FN_KINDS: &[&str] = &["function_definition"];

pub fn run(path: &Path, source: &str, tree: &Tree) -> Vec<Finding> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    walk(tree.root_node(), &mut |node| {
        if node.kind() != "catch_clause" {
            return;
        }
        let empty = node
            .child_by_field_name("body")
            .is_some_and(|b| b.named_child_count() == 0);
        if empty {
            out.push(item_finding(
                path,
                node,
                bytes,
                FN_KINDS,
                "cpp.empty-catch",
                Severity::Medium,
                "Empty `catch` swallows the exception",
                "Handle the exception, rethrow it, or at least log it. If ignoring it is deliberate, \
                 say why in a comment inside the block.",
            ));
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        let tree = dslp_base::parse("cpp", src).unwrap();
        run(Path::new("a.cpp"), src, &tree).len()
    }

    #[test]
    fn empty_catch_fires_but_handled_or_commented_does_not() {
        assert_eq!(count("void f() { try { g(); } catch (...) {} }"), 1);
        assert_eq!(
            count("void f() { try { g(); } catch (const E& e) { log(e); } }"),
            0
        );
        assert_eq!(
            count("void f() { try { g(); } catch (...) { /* best effort */ } }"),
            0
        );
    }
}
