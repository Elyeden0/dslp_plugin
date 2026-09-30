//! Habits common in AI-written Python: silent `except`, leftover debugger hooks.

use std::path::Path;

use dslp_core::{Finding, Severity};
use dslp_lang_common::{item_finding, walk};
use tree_sitter::{Node, Tree};

const FN_KINDS: &[&str] = &["function_definition"];
const DEBUG_CALLS: &[&str] = &["breakpoint", "pdb.set_trace", "ipdb.set_trace"];

pub fn run(path: &Path, source: &str, tree: &Tree) -> Vec<Finding> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    walk(tree.root_node(), &mut |node| match node.kind() {
        "except_clause" if is_silent(node) => out.push(item_finding(
            path,
            node,
            bytes,
            FN_KINDS,
            "python.silent-except",
            Severity::Medium,
            "`except` block does nothing, silently swallowing the error",
            "Handle or log the exception, narrow it to the case you expect, or re-raise. \
             `contextlib.suppress(...)` states an intentional ignore.",
        )),
        "call" => {
            let callee = node
                .child_by_field_name("function")
                .and_then(|f| f.utf8_text(bytes).ok());
            if callee.is_some_and(|c| DEBUG_CALLS.contains(&c)) {
                out.push(item_finding(
                    path,
                    node,
                    bytes,
                    FN_KINDS,
                    "python.leftover-debug",
                    Severity::Low,
                    &format!("`{}()` call left in code", callee.unwrap_or_default()),
                    "Remove it before committing.",
                ));
            }
        }
        _ => {}
    });
    out
}

/// A block made only of `pass` / `...`.
fn is_silent(except: Node) -> bool {
    let mut cursor = except.walk();
    let Some(block) = except.children(&mut cursor).find(|c| c.kind() == "block") else {
        return false;
    };
    let mut inner = block.walk();
    let mut stmts = block.named_children(&mut inner).peekable();
    stmts.peek().is_some()
        && stmts.all(|s| {
            s.kind() == "pass_statement"
                || (s.kind() == "expression_statement"
                    && s.child(0).is_some_and(|c| c.kind() == "ellipsis"))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detectors(src: &str) -> Vec<String> {
        let tree = dslp_base::parse("python", src).unwrap();
        run(Path::new("a.py"), src, &tree)
            .into_iter()
            .map(|f| f.detector)
            .collect()
    }

    #[test]
    fn except_pass_fires_but_handled_does_not() {
        assert_eq!(
            detectors("try:\n    f()\nexcept Exception:\n    pass\n"),
            ["python.silent-except"]
        );
        assert_eq!(
            detectors("try:\n    f()\nexcept:\n    ...\n"),
            ["python.silent-except"]
        );
        assert!(detectors("try:\n    f()\nexcept ValueError:\n    log(1)\n").is_empty());
    }

    #[test]
    fn debugger_hooks_fire_but_print_does_not() {
        assert_eq!(
            detectors("def f():\n    breakpoint()\n    pdb.set_trace()\n"),
            ["python.leftover-debug"; 2]
        );
        assert!(detectors("print('hi')\n").is_empty());
    }
}
