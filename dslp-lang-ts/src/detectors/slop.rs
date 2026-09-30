//! Habits common in AI-written TS/JS: swallowed errors and leftover debugging.

use std::path::Path;

use dslp_core::{Finding, Severity};
use dslp_lang_common::{item_finding, walk};
use tree_sitter::Tree;

const FN_KINDS: &[&str] = &["function_declaration", "method_definition", "function_expression"];
const DEBUG_CALLS: &[&str] = &["console.log", "console.debug", "console.trace"];

pub fn run(path: &Path, source: &str, tree: &Tree) -> Vec<Finding> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    walk(tree.root_node(), &mut |node| match node.kind() {
        "catch_clause" => {
            let empty = node.child_by_field_name("body").is_some_and(|b| b.named_child_count() == 0);
            if empty {
                out.push(item_finding(
                    path, node, bytes, FN_KINDS, "ts.empty-catch", Severity::Medium,
                    "Empty `catch` swallows the error",
                    "Handle the error, rethrow it, or at least log it with context. If ignoring it is \
                     deliberate, say why in a comment inside the block.",
                ));
            }
        }
        "debugger_statement" => out.push(item_finding(
            path, node, bytes, FN_KINDS, "ts.leftover-debug", Severity::Low,
            "`debugger` statement left in code",
            "Remove it before committing.",
        )),
        "call_expression" => {
            let callee = node.child_by_field_name("function").and_then(|f| f.utf8_text(bytes).ok());
            if callee.is_some_and(|c| DEBUG_CALLS.contains(&c)) {
                out.push(item_finding(
                    path, node, bytes, FN_KINDS, "ts.leftover-debug", Severity::Low,
                    &format!("`{}` call left in code", callee.unwrap_or_default()),
                    "Remove it, or route real logging through your logger.",
                ));
            }
        }
        _ => {}
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detectors(src: &str) -> Vec<String> {
        let tree = dslp_base::parse("javascript", src).unwrap();
        run(Path::new("a.js"), src, &tree).into_iter().map(|f| f.detector).collect()
    }

    #[test]
    fn empty_catch_fires_but_handled_or_commented_does_not() {
        assert_eq!(detectors("try { f(); } catch (e) {}"), ["ts.empty-catch"]);
        assert!(detectors("try { f(); } catch (e) { log(e); }").is_empty());
        assert!(detectors("try { f(); } catch (e) { /* ignore: best effort */ }").is_empty());
    }

    #[test]
    fn debug_leftovers_fire_but_console_error_does_not() {
        assert_eq!(detectors("function f() { console.log(1); debugger; }"), ["ts.leftover-debug"; 2]);
        assert!(detectors("console.error('boom'); console.warn('w');").is_empty());
    }

    #[test]
    fn fingerprints_ignore_line_shifts() {
        let src = "function f() { console.log(1); }";
        let tree = dslp_base::parse("javascript", src).unwrap();
        let a = run(Path::new("a.js"), src, &tree);
        let shifted = format!("\n\n{src}");
        let tree = dslp_base::parse("javascript", &shifted).unwrap();
        let b = run(Path::new("a.js"), &shifted, &tree);
        assert_eq!(a[0].fingerprint, b[0].fingerprint);
    }
}
