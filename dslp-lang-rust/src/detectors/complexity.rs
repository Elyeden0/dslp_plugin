use std::path::Path;

use dslp_base::ComplexityConfig;
use dslp_core::{Finding, Severity};
use dslp_lang_common::{cyclomatic, finding_at_node, node_length_lines, param_count, params_finding};
use tree_sitter::{Node, Tree};

/// Rust decision-point node kinds for cyclomatic complexity.
const DECISION_KINDS: &[&str] = &[
    "if_expression",
    "while_expression",
    "for_expression",
    "loop_expression",
    "try_expression",
    "match_arm",
    "&&",
    "||",
];

/// Per-function cyclomatic complexity + length. The walk and name extraction
/// are Rust-specific; the counting/length/finding machinery is shared.
pub fn run(path: &Path, source: &str, tree: &Tree, cfg: &ComplexityConfig) -> Vec<Finding> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    walk_functions(tree.root_node(), bytes, &mut |fn_node, name| {
        let body = fn_node.child_by_field_name("body").unwrap_or(fn_node);
        let cx = cyclomatic(body, DECISION_KINDS);
        let length = node_length_lines(fn_node);

        if cx >= cfg.cyclomatic_threshold {
            out.push(finding_at_node(
                path,
                fn_node,
                "rust.complexity",
                Severity::Medium,
                format!(
                    "Function `{}` has cyclomatic complexity {} (threshold {}). \
                     Consider extracting helpers or simplifying control flow.",
                    name, cx, cfg.cyclomatic_threshold
                ),
                "Extract a sub-routine for the deepest branch, or replace a chain \
                 of `if`s with a `match` over an enum.",
                &format!("complexity:{}", name),
            ));
        }
        if length >= cfg.length_threshold {
            out.push(finding_at_node(
                path,
                fn_node,
                "rust.length",
                Severity::Low,
                format!(
                    "Function `{}` is {} lines (threshold {}). Long functions tend \
                     to conflate concerns.",
                    name, length, cfg.length_threshold
                ),
                "Identify the most independent block (often setup or post-processing) \
                 and extract it.",
                &format!("length:{}", name),
            ));
        }
        let params = fn_node.child_by_field_name("parameters").map_or(0, param_count);
        out.extend(params_finding(path, fn_node, "rust.params", name, params, cfg.param_threshold));
    });
    out
}

fn walk_functions<'a, F: FnMut(Node<'a>, &str)>(node: Node<'a>, bytes: &[u8], f: &mut F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_item" {
            if let Some(name_node) = child.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(bytes) {
                    f(child, name);
                }
            }
        }
        walk_functions(child, bytes, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Tree {
        dslp_base::parse("rust", src).unwrap()
    }

    fn cfg() -> ComplexityConfig {
        ComplexityConfig {
            cyclomatic_threshold: 4,
            length_threshold: 8,
            param_threshold: 3,
        }
    }

    #[test]
    fn simple_fn_does_not_fire() {
        let src = "fn ok() { let x = 1; let y = x + 1; }";
        let t = parse(src);
        assert!(run(Path::new("a.rs"), src, &t, &cfg()).is_empty());
    }

    #[test]
    fn complex_fn_fires_complexity() {
        let src = r#"
fn tangle(x: i32) -> i32 {
    if x > 0 {
        if x < 10 { return x; } else { return x * 2; }
    }
    while x < 0 {
        if x == -1 { break; }
    }
    for i in 0..10 {
        if i % 2 == 0 { continue; }
    }
    x
}
"#;
        let t = parse(src);
        let r = run(Path::new("a.rs"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "rust.complexity"), "got: {:?}", r);
    }

    #[test]
    fn long_fn_fires_length() {
        let body: String = (0..15).map(|i| format!("    let x{} = {};\n", i, i)).collect();
        let src = format!("fn long() {{\n{}}}\n", body);
        let t = parse(&src);
        let r = run(Path::new("a.rs"), &src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "rust.length"), "got: {:?}", r);
    }

    #[test]
    fn many_params_fire_but_self_is_free() {
        let many = "fn f(a: i32, b: i32, c: i32) {}";
        let r = run(Path::new("a.rs"), many, &parse(many), &cfg());
        assert!(r.iter().any(|f| f.detector == "rust.params"), "got: {:?}", r);
        let method = "impl S { fn f(&self, a: i32, b: i32) {} }";
        let r = run(Path::new("a.rs"), method, &parse(method), &cfg());
        assert!(r.iter().all(|f| f.detector != "rust.params"), "got: {:?}", r);
    }
}
