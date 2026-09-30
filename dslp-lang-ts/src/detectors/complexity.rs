//! Cyclomatic + length per function for TypeScript / JavaScript.
//!
//! tree-sitter-typescript and tree-sitter-javascript share most node kinds.
//! The walk + name extraction are TS-specific; counting/length/finding are
//! shared via `dslp-lang-common`.

use std::path::Path;

use dslp_base::ComplexityConfig;
use dslp_core::{Finding, Severity};
use dslp_lang_common::{
    cyclomatic, finding_at_node, node_length_lines, param_count, params_finding,
};
use tree_sitter::{Node, Tree};

/// `switch_case` = each `case` is a path (`default` is `switch_default`,
/// excluded). `&&`/`||`/`??` short-circuit. `catch_clause` adds a path.
const DECISION_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "for_of_statement",
    "while_statement",
    "do_statement",
    "ternary_expression",
    "catch_clause",
    "switch_case",
    "&&",
    "||",
    "??",
];

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
                "ts.complexity",
                Severity::Medium,
                format!(
                    "Function `{}` has cyclomatic complexity {} (threshold {}). \
                     Consider extracting helpers or simplifying control flow.",
                    name, cx, cfg.cyclomatic_threshold
                ),
                "Pull out an early-return guard, or extract the deepest branch into \
                 its own function.",
                &format!("ts.complexity:{}", name),
            ));
        }
        if length >= cfg.length_threshold {
            out.push(finding_at_node(
                path,
                fn_node,
                "ts.length",
                Severity::Low,
                format!(
                    "Function `{}` is {} lines (threshold {}). Long functions tend to \
                     conflate concerns.",
                    name, length, cfg.length_threshold
                ),
                "Identify the most independent block (often setup or post-processing) \
                 and extract it.",
                &format!("ts.length:{}", name),
            ));
        }
        let params = fn_node
            .child_by_field_name("parameters")
            .map_or(0, param_count);
        out.extend(params_finding(
            path,
            fn_node,
            "ts.params",
            name,
            params,
            cfg.param_threshold,
        ));
    });
    out
}

fn walk_functions<'a, F: FnMut(Node<'a>, &str)>(node: Node<'a>, bytes: &[u8], f: &mut F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_function_kind(child.kind()) {
            let name = function_name(child, bytes).unwrap_or_else(|| "<anonymous>".to_string());
            f(child, &name);
        }
        walk_functions(child, bytes, f);
    }
}

fn is_function_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
            | "function"
    )
}

fn function_name(fn_node: Node, bytes: &[u8]) -> Option<String> {
    if let Some(name_node) = fn_node.child_by_field_name("name") {
        if let Ok(text) = name_node.utf8_text(bytes) {
            return Some(text.to_string());
        }
    }
    // For `const x = () => ...` the parent variable_declarator carries the name.
    let parent = fn_node.parent()?;
    if parent.kind() == "variable_declarator" || parent.kind() == "pair" {
        if let Some(name_node) = parent.child_by_field_name("name") {
            if let Ok(text) = name_node.utf8_text(bytes) {
                return Some(text.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ComplexityConfig {
        ComplexityConfig {
            cyclomatic_threshold: 4,
            length_threshold: 8,
            param_threshold: 3,
        }
    }

    fn parse(lang: &str, src: &str) -> Tree {
        dslp_base::parse(lang, src).unwrap()
    }

    #[test]
    fn simple_ts_fn_does_not_fire() {
        let src = "function ok(x: number): number { return x + 1; }";
        let t = parse("typescript", src);
        assert!(run(Path::new("a.ts"), src, &t, &cfg()).is_empty());
    }

    #[test]
    fn complex_ts_fn_fires_complexity() {
        let src = r#"
function tangle(x: number): number {
    if (x > 0) {
        if (x < 10) return x;
        if (x < 20) return x * 2;
    }
    while (x > 100) {
        if (x % 2 === 0) return x;
    }
    for (let i = 0; i < 10; i++) {
        if (i > 5) return i;
    }
    return x;
}
"#;
        let t = parse("typescript", src);
        let r = run(Path::new("a.ts"), src, &t, &cfg());
        assert!(
            r.iter().any(|f| f.detector == "ts.complexity"),
            "got: {:?}",
            r
        );
    }

    #[test]
    fn arrow_fn_named_via_const() {
        let src = r#"
const tangle = (x: number): number => {
    if (x > 0) {
        if (x < 10) return x;
        if (x < 20) return x * 2;
        if (x < 30) return x * 3;
    }
    return x;
};
"#;
        let t = parse("typescript", src);
        let r = run(Path::new("a.ts"), src, &t, &cfg());
        assert!(
            r.iter().any(|f| f.message.contains("`tangle`")),
            "got: {:?}",
            r
        );
    }

    #[test]
    fn long_fn_fires_length() {
        let body: String = (0..15)
            .map(|i| format!("    let x{} = {};\n", i, i))
            .collect();
        let src = format!("function long() {{\n{}}}\n", body);
        let t = parse("typescript", &src);
        let r = run(Path::new("a.ts"), &src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "ts.length"), "got: {:?}", r);
    }

    #[test]
    fn many_params_fire() {
        let src = "function f(a, b, c) {}";
        let t = parse("javascript", src);
        let r = run(Path::new("a.js"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "ts.params"), "got: {:?}", r);
    }
}
