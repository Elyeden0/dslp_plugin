//! Cyclomatic + length per function for C++.
//!
//! Same decision-point set as C plus C++ specifics: `try_statement` /
//! `catch_clause` (each `catch` is a path), and lambda function bodies are
//! walked too.

use std::path::Path;

use dslp_base::ComplexityConfig;
use dslp_core::{Finding, Severity};
use dslp_lang_common::{cyclomatic, finding_at_node, node_length_lines};
use tree_sitter::{Node, Tree};

/// C++ decision-point kinds: C's set plus `for_range_loop` and `catch_clause`.
const DECISION_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_range_loop",
    "while_statement",
    "do_statement",
    "case_statement",
    "conditional_expression",
    "catch_clause",
    "&&",
    "||",
];

pub fn run(
    path: &Path,
    source: &str,
    tree: &Tree,
    _lang_prefix: &'static str,
    cfg: &ComplexityConfig,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    walk_functions(tree.root_node(), bytes, &mut |fn_node, name| {
        let body = fn_node.child_by_field_name("body").unwrap_or(fn_node);
        let cx = cyclomatic(body, DECISION_KINDS);
        let length = node_length_lines(fn_node);
        if cx >= cfg.cyclomatic_threshold {
            out.push(complexity_finding(path, fn_node, name, cx, cfg));
        }
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
        "cpp.complexity",
        Severity::Medium,
        format!(
            "Function `{}` has cyclomatic complexity {} (threshold {}). \
             Consider extracting helpers or simplifying control flow.",
            name, cx, cfg.cyclomatic_threshold
        ),
        "Pull deep `if`/`switch` chains into named helpers; consider replacing \
         a long branchy method with `std::variant` + `std::visit`.",
        &format!("cpp.complexity:{}", name),
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
        "cpp.length",
        Severity::Low,
        format!(
            "Function `{}` is {} lines (threshold {}). Long functions tend to \
             conflate concerns.",
            name, length, cfg.length_threshold
        ),
        "Identify the most independent block and extract it. RAII guards often \
         let you collapse setup/teardown into a single declaration.",
        &format!("cpp.length:{}", name),
    )
}

fn walk_functions<'a, F: FnMut(Node<'a>, &str)>(node: Node<'a>, bytes: &[u8], f: &mut F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_definition" {
            if let Some(name) = cpp_function_name(child, bytes) {
                f(child, &name);
            }
        }
        walk_functions(child, bytes, f);
    }
}

fn cpp_function_name(fn_node: Node, bytes: &[u8]) -> Option<String> {
    let decl = fn_node.child_by_field_name("declarator")?;
    extract_declarator_identifier(decl, bytes)
}

fn extract_declarator_identifier(node: Node, bytes: &[u8]) -> Option<String> {
    if let Some(direct) = direct_identifier(node, bytes) {
        return Some(direct);
    }
    if is_wrapping_declarator(node.kind()) {
        return extract_declarator_identifier(node.child_by_field_name("declarator")?, bytes);
    }
    if is_compound_name(node.kind()) {
        return last_child_identifier(node, bytes);
    }
    first_descendant_identifier(node, bytes)
}

fn direct_identifier(node: Node, bytes: &[u8]) -> Option<String> {
    matches!(node.kind(), "identifier" | "field_identifier")
        .then(|| node.utf8_text(bytes).ok().map(String::from))
        .flatten()
}

fn is_wrapping_declarator(kind: &str) -> bool {
    matches!(
        kind,
        "function_declarator"
            | "pointer_declarator"
            | "parenthesized_declarator"
            | "reference_declarator"
            | "rvalue_reference_declarator"
    )
}

fn is_compound_name(kind: &str) -> bool {
    matches!(kind, "qualified_identifier" | "operator_name" | "destructor_name")
}

fn last_child_identifier(node: Node, bytes: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let last = node.named_children(&mut cursor).last()?;
    match last.kind() {
        "identifier" | "field_identifier" => last.utf8_text(bytes).ok().map(String::from),
        _ => extract_declarator_identifier(last, bytes),
    }
}

fn first_descendant_identifier(node: Node, bytes: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if let Some(s) = extract_declarator_identifier(child, bytes) {
            return Some(s);
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
        }
    }

    fn parse_cpp(src: &str) -> Tree {
        dslp_base::parse("cpp", src).unwrap()
    }

    #[test]
    fn simple_cpp_fn_does_not_fire() {
        let src = "#include <cstdint>\nint ok(int x) { return x + 1; }\n";
        let t = parse_cpp(src);
        let r = run(Path::new("a.cpp"), src, &t, "cpp", &cfg());
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn complex_cpp_fn_fires() {
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
        let t = parse_cpp(src);
        let r = run(Path::new("a.cpp"), src, &t, "cpp", &cfg());
        assert!(
            r.iter().any(|f| f.detector == "cpp.complexity"),
            "expected cpp.complexity, got: {:?}",
            r
        );
    }

    #[test]
    fn try_catch_counts_paths() {
        let src = r#"
#include <stdexcept>

int safe_div(int a, int b) {
    try {
        if (b == 0) throw std::runtime_error("zero");
        if (a == 0) return 0;
        return a / b;
    } catch (const std::runtime_error&) {
        return -1;
    } catch (const std::exception&) {
        return -2;
    } catch (...) {
        return -3;
    }
}
"#;
        let t = parse_cpp(src);
        let r = run(Path::new("a.cpp"), src, &t, "cpp", &cfg());
        assert!(
            r.iter().any(|f| f.detector == "cpp.complexity"),
            "expected catch clauses to drive complexity, got: {:?}",
            r
        );
    }

    #[test]
    fn range_for_counts() {
        let src = r#"
#include <vector>

int sum_positive(const std::vector<int>& xs) {
    int total = 0;
    for (int x : xs) {
        if (x > 0) total += x;
        if (x > 10) total += x;
        if (x > 100) total += x;
        if (x > 1000) total += x;
    }
    return total;
}
"#;
        let t = parse_cpp(src);
        let r = run(Path::new("a.cpp"), src, &t, "cpp", &cfg());
        assert!(
            r.iter().any(|f| f.detector == "cpp.complexity"),
            "expected range-for to participate in complexity, got: {:?}",
            r
        );
    }
}
