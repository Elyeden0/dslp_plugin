//! Habits common in AI-written Go: results thrown away with the blank identifier.

use std::path::Path;

use dslp_core::{Finding, Severity};
use dslp_lang_common::{item_finding, walk};
use tree_sitter::{Node, Tree};

const FN_KINDS: &[&str] = &["function_declaration", "method_declaration"];

pub fn run(path: &Path, source: &str, tree: &Tree) -> Vec<Finding> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    walk(tree.root_node(), &mut |node| {
        if node.kind() == "assignment_statement" && discards_call_result(node, bytes) {
            out.push(item_finding(
                path,
                node,
                bytes,
                FN_KINDS,
                "go.ignored-error",
                Severity::Medium,
                "Call result assigned to `_`, discarding a possible error",
                "Check the error and return or log it. If ignoring it is deliberate, say why in a comment.",
            ));
        }
    });
    out
}

/// `_ = f()` or `_, _ = f()`: every target is the blank identifier and the
/// right-hand side is a call.
fn discards_call_result(assign: Node, bytes: &[u8]) -> bool {
    let (Some(left), Some(right)) = (
        assign.child_by_field_name("left"),
        assign.child_by_field_name("right"),
    ) else {
        return false;
    };
    let mut lc = left.walk();
    let all_blank = left.named_child_count() > 0
        && left
            .named_children(&mut lc)
            .all(|n| n.utf8_text(bytes) == Ok("_"));
    let mut rc = right.walk();
    let has_call = right
        .named_children(&mut rc)
        .any(|n| n.kind() == "call_expression");
    all_blank && has_call
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(src: &str) -> usize {
        let tree = dslp_base::parse("go", src).unwrap();
        run(Path::new("a.go"), src, &tree).len()
    }

    #[test]
    fn blank_assigned_calls_fire() {
        assert_eq!(
            count("package p\n\nfunc f() {\n\t_ = g()\n\t_, _ = h()\n}\n"),
            2
        );
    }

    #[test]
    fn real_assignments_do_not_fire() {
        assert_eq!(
            count("package p\n\nfunc f() {\n\tx = g()\n\t_ = 5\n\t_, y = h()\n}\n"),
            0
        );
    }
}
