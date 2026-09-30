//! Cyclomatic + length per function for C.
//!
//! Decision-point set: `if_statement`, `for_statement`, `while_statement`,
//! `do_statement`, `case_statement`, `conditional_expression` (ternary `?:`),
//! plus `&&` / `||`. Preprocessor conditionals (`#ifdef` / `#if`) are NOT
//! counted — they're conditional compilation, not control flow at the
//! complexity-of-the-running-function level.

use std::path::Path;

use dslp_base::ComplexityConfig;
use dslp_core::{Finding, Severity};
use dslp_lang_common::{
    c_family_params, cyclomatic, finding_at_node, max_nesting, nesting_finding, node_length_lines,
    param_count, params_finding,
};
use tree_sitter::{Node, Tree};

/// C decision-point kinds. Preprocessor conditionals (`#ifdef`/`#if`) are
/// intentionally excluded — conditional compilation is a different axis.
const DECISION_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "while_statement",
    "do_statement",
    "case_statement",
    "conditional_expression",
    "&&",
    "||",
];

/// Control-flow nodes that add a nesting level.
const NESTING_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "while_statement",
    "do_statement",
    "switch_statement",
];
/// Parents whose `if` child is an `else if`, not a deeper level.
const ELSE_PARENTS: &[&str] = &["else_clause", "if_statement"];

pub fn run(path: &Path, source: &str, tree: &Tree, cfg: &ComplexityConfig) -> Vec<Finding> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    walk_functions(tree.root_node(), bytes, &mut |fn_node, name| {
        let body = fn_node.child_by_field_name("body").unwrap_or(fn_node);
        let cx = cyclomatic(body, DECISION_KINDS);
        let length = node_length_lines(fn_node);
        if cx >= cfg.cyclomatic_threshold {
            out.push(complexity_finding(path, fn_node, name, cx, cfg));
        }
        let params = c_family_params(fn_node).map_or(0, param_count);
        out.extend(params_finding(
            path,
            fn_node,
            "c.params",
            name,
            params,
            cfg.param_threshold,
        ));
        let depth = max_nesting(body, NESTING_KINDS, ELSE_PARENTS);
        out.extend(nesting_finding(
            path,
            fn_node,
            "c.nesting",
            name,
            depth,
            cfg.nesting_threshold,
        ));
        if length >= cfg.length_threshold {
            out.push(length_finding(path, fn_node, name, length, cfg));
        }
    });
    out
}

fn complexity_finding(
    path: &Path,
    fn_node: Node,
    name: &str,
    cx: u32,
    cfg: &ComplexityConfig,
) -> Finding {
    finding_at_node(
        path,
        fn_node,
        "c.complexity",
        Severity::Medium,
        format!(
            "Function `{}` has cyclomatic complexity {} (threshold {}). \
             Consider extracting helpers or simplifying control flow.",
            name, cx, cfg.cyclomatic_threshold
        ),
        "Replace deep `if`/`switch` chains with table dispatch, or extract \
         early-return helpers.",
        &format!("c.complexity:{}", name),
    )
}

fn length_finding(
    path: &Path,
    fn_node: Node,
    name: &str,
    length: u32,
    cfg: &ComplexityConfig,
) -> Finding {
    finding_at_node(
        path,
        fn_node,
        "c.length",
        Severity::Low,
        format!(
            "Function `{}` is {} lines (threshold {}). Long functions tend to \
             conflate concerns.",
            name, length, cfg.length_threshold
        ),
        "Identify the most independent block (often setup or post-processing) \
         and extract it.",
        &format!("c.length:{}", name),
    )
}

fn walk_functions<'a, F: FnMut(Node<'a>, &str)>(node: Node<'a>, bytes: &[u8], f: &mut F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_definition" {
            if let Some(name) = dslp_lang_common::c_family_function_name(child, bytes) {
                f(child, &name);
            }
        }
        walk_functions(child, bytes, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ComplexityConfig {
        ComplexityConfig {
            cyclomatic_threshold: 4,
            length_threshold: 8,
            param_threshold: 3,
            nesting_threshold: 3,
        }
    }

    fn parse_c(src: &str) -> Tree {
        dslp_base::parse("c", src).unwrap()
    }

    #[test]
    fn simple_c_fn_does_not_fire() {
        let src = "int ok(int x) { return x + 1; }";
        let t = parse_c(src);
        let r = run(Path::new("a.c"), src, &t, &cfg());
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn complex_c_fn_fires() {
        let src = r#"
int tangle(int x) {
    if (x > 0) {
        if (x < 10) return x;
        if (x < 20) return x * 2;
    }
    for (int i = 0; i < 100; i++) {
        if (i > 50) return i;
        if (i > 60) return i + 1;
    }
    return x;
}
"#;
        let t = parse_c(src);
        let r = run(Path::new("a.c"), src, &t, &cfg());
        assert!(
            r.iter().any(|f| f.detector == "c.complexity"),
            "expected c.complexity, got: {:?}",
            r
        );
    }

    #[test]
    fn switch_cases_count() {
        let src = r#"
const char *classify(int x) {
    switch (x) {
        case 1: return "one";
        case 2: return "two";
        case 3: return "three";
        default: return "other";
    }
}
"#;
        let t = parse_c(src);
        let r = run(Path::new("a.c"), src, &t, &cfg());
        assert!(
            r.iter().any(|f| f.detector == "c.complexity"),
            "expected switch to drive complexity, got: {:?}",
            r
        );
    }

    #[test]
    fn pointer_returning_fn_name_extracted() {
        // Pointer-declarator wraps the identifier — make sure name extraction
        // still finds `make_buffer`.
        let src = r#"
int *make_buffer(int n) {
    if (n < 0) return 0;
    if (n < 10) return 0;
    if (n < 20) return 0;
    if (n < 30) return 0;
    if (n < 40) return 0;
    return 0;
}
"#;
        let t = parse_c(src);
        let r = run(Path::new("a.c"), src, &t, &cfg());
        assert!(
            r.iter().any(|f| f.message.contains("`make_buffer`")),
            "expected name in message, got: {:?}",
            r
        );
    }

    #[test]
    fn many_params_fire_including_through_pointer_return_types() {
        let src = "int *f(int a, int b, int c) { return 0; }";
        let r = run(Path::new("a.c"), src, &parse_c(src), &cfg());
        assert!(r.iter().any(|f| f.detector == "c.params"), "got: {:?}", r);
        let ok = "int f(void) { return 0; }";
        assert!(run(Path::new("a.c"), ok, &parse_c(ok), &cfg()).is_empty());
    }

    #[test]
    fn deep_nesting_fires_but_else_if_chains_do_not() {
        let deep = "void f(int x) { if (x) { for (;;) { while (x) { return; } } } }";
        let r = run(Path::new("a.c"), deep, &parse_c(deep), &cfg());
        assert!(r.iter().any(|f| f.detector == "c.nesting"), "got: {:?}", r);
        let flat = "void f(int x) { if (x == 1) {} else if (x == 2) {} else if (x == 3) {} else if (x == 4) {} }";
        let r = run(Path::new("a.c"), flat, &parse_c(flat), &cfg());
        assert!(r.iter().all(|f| f.detector != "c.nesting"), "got: {:?}", r);
    }
}
