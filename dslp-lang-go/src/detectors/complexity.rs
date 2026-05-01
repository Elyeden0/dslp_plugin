//! Cyclomatic + length per function for Go. Walk + name extraction are
//! Go-specific; counting/length/finding are shared via `dslp-lang-common`.

use std::path::Path;

use dslp_base::ComplexityConfig;
use dslp_core::{Finding, Severity};
use dslp_lang_common::{cyclomatic, finding_at_node, node_length_lines};
use tree_sitter::{Node, Tree};

/// Go decision-point kinds: switch `expression_case`/`default_case`, select
/// `communication_case`, type-switch `type_case`, plus the loop/branch nodes
/// and short-circuit operators.
const DECISION_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "expression_case",
    "default_case",
    "communication_case",
    "type_case",
    "&&",
    "||",
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
                "go.complexity",
                Severity::Medium,
                format!(
                    "Function `{}` has cyclomatic complexity {} (threshold {}). \
                     Consider extracting helpers or simplifying control flow.",
                    name, cx, cfg.cyclomatic_threshold
                ),
                "Replace deep `if`/`switch` chains with table-driven dispatch \
                 or extract early-return helpers.",
                &format!("go.complexity:{}:{}", name, cx),
            ));
        }
        if length >= cfg.length_threshold {
            out.push(finding_at_node(
                path,
                fn_node,
                "go.length",
                Severity::Low,
                format!(
                    "Function `{}` is {} lines (threshold {}). Long functions tend to \
                     conflate concerns.",
                    name, length, cfg.length_threshold
                ),
                "Identify the most independent block (often setup or post-processing) \
                 and extract it into a helper.",
                &format!("go.length:{}:{}", name, length),
            ));
        }
    });
    out
}

fn walk_functions<'a, F: FnMut(Node<'a>, &str)>(node: Node<'a>, bytes: &[u8], f: &mut F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "function_declaration" | "method_declaration") {
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

    fn cfg() -> ComplexityConfig {
        ComplexityConfig {
            cyclomatic_threshold: 4,
            length_threshold: 8,
        }
    }

    fn parse(src: &str) -> Tree {
        dslp_base::parse("go", src).unwrap()
    }

    #[test]
    fn simple_go_fn_does_not_fire() {
        let src = "package main\nfunc ok(x int) int { return x + 1 }\n";
        let t = parse(src);
        assert!(run(Path::new("a.go"), src, &t, &cfg()).is_empty());
    }

    #[test]
    fn complex_go_fn_fires() {
        let src = r#"package main

func tangle(x int) int {
    if x > 0 {
        if x < 10 { return x }
        if x < 20 { return x * 2 }
    }
    for i := 0; i < 100; i++ {
        if i > 50 { return i }
        if i > 60 { return i + 1 }
    }
    return x
}
"#;
        let t = parse(src);
        let r = run(Path::new("a.go"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "go.complexity"), "got: {:?}", r);
    }

    #[test]
    fn switch_cases_count() {
        let src = r#"package main

func classify(x int) string {
    switch x {
    case 1: return "one"
    case 2: return "two"
    case 3: return "three"
    default: return "other"
    }
}
"#;
        let t = parse(src);
        let r = run(Path::new("a.go"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "go.complexity"), "got: {:?}", r);
    }

    #[test]
    fn long_go_fn_fires_length() {
        let body: String = (0..15).map(|i| format!("    x{} := {}\n", i, i)).collect();
        let src = format!("package main\n\nfunc long() {{\n{}}}\n", body);
        let t = parse(&src);
        let r = run(Path::new("a.go"), &src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "go.length"), "got: {:?}", r);
    }

    #[test]
    fn method_declaration_works() {
        let src = r#"package main

type Foo struct{}

func (f *Foo) tangle(x int) int {
    if x > 0 {
        if x < 10 { return x }
        if x < 20 { return x * 2 }
        if x < 30 { return x * 3 }
    }
    return x
}
"#;
        let t = parse(src);
        let r = run(Path::new("a.go"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "go.complexity"), "got: {:?}", r);
    }
}
