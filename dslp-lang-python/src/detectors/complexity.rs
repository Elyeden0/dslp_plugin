//! Cyclomatic + length per function for Python. Walk + name extraction are
//! Python-specific; counting/length/finding are shared via `dslp-lang-common`.

use std::path::Path;

use dslp_base::ComplexityConfig;
use dslp_core::{Finding, Severity};
use dslp_lang_common::{cyclomatic, finding_at_node, node_length_lines, param_count, params_finding};
use tree_sitter::{Node, Tree};

/// Python decision-point kinds: `elif_clause` and `case_clause` (PEP 634
/// match) each add a path; `conditional_expression` is `x if c else y`;
/// `and`/`or` are the boolean-operator tokens.
const DECISION_KINDS: &[&str] = &[
    "if_statement",
    "elif_clause",
    "for_statement",
    "while_statement",
    "except_clause",
    "case_clause",
    "conditional_expression",
    "and",
    "or",
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
                "python.complexity",
                Severity::Medium,
                format!(
                    "Function `{}` has cyclomatic complexity {} (threshold {}). \
                     Consider extracting helpers or simplifying control flow.",
                    name, cx, cfg.cyclomatic_threshold
                ),
                "Replace deep `if`/`elif` chains with a dispatch dict or a `match` \
                 statement; pull out the deepest branch into its own function.",
                &format!("python.complexity:{}", name),
            ));
        }
        if length >= cfg.length_threshold {
            out.push(finding_at_node(
                path,
                fn_node,
                "python.length",
                Severity::Low,
                format!(
                    "Function `{}` is {} lines (threshold {}). Long functions tend to \
                     conflate concerns.",
                    name, length, cfg.length_threshold
                ),
                "Identify the most independent block (often setup or post-processing) \
                 and extract it into a helper.",
                &format!("python.length:{}", name),
            ));
        }
        let params = fn_node
            .child_by_field_name("parameters")
            .map_or(0, |p| param_count(p) - receiver_count(p, bytes));
        out.extend(params_finding(path, fn_node, "python.params", name, params, cfg.param_threshold));
    });
    out
}

/// 1 if the first parameter is the conventional `self`/`cls` receiver.
fn receiver_count(params: Node, bytes: &[u8]) -> u32 {
    let first = params.named_child(0).filter(|n| n.kind() == "identifier");
    let is_receiver = first.and_then(|n| n.utf8_text(bytes).ok()).is_some_and(|t| t == "self" || t == "cls");
    u32::from(is_receiver)
}

fn walk_functions<'a, F: FnMut(Node<'a>, &str)>(node: Node<'a>, bytes: &[u8], f: &mut F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "function_definition" {
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
            param_threshold: 3,
        }
    }

    fn parse(src: &str) -> Tree {
        dslp_base::parse("python", src).unwrap()
    }

    #[test]
    fn simple_python_fn_does_not_fire() {
        let src = "def ok(x):\n    return x + 1\n";
        let t = parse(src);
        assert!(run(Path::new("a.py"), src, &t, &cfg()).is_empty());
    }

    #[test]
    fn complex_python_fn_fires() {
        let src = r#"
def tangle(x):
    if x > 0:
        if x < 10:
            return x
        elif x < 20:
            return x * 2
    while x > 100:
        if x % 2 == 0:
            return x
        if x % 3 == 0:
            return x
    return x
"#;
        let t = parse(src);
        let r = run(Path::new("a.py"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "python.complexity"), "got: {:?}", r);
    }

    #[test]
    fn match_case_counts() {
        let src = r#"
def classify(x):
    match x:
        case 1:
            return "one"
        case 2:
            return "two"
        case 3:
            return "three"
        case _:
            return "other"
"#;
        let t = parse(src);
        let r = run(Path::new("a.py"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "python.complexity"), "got: {:?}", r);
    }

    #[test]
    fn long_python_fn_fires_length() {
        let body: String = (0..15).map(|i| format!("    x{} = {}\n", i, i)).collect();
        let src = format!("def long():\n{}", body);
        let t = parse(&src);
        let r = run(Path::new("a.py"), &src, &t, &cfg());
        assert!(r.iter().any(|f| f.detector == "python.length"), "got: {:?}", r);
    }

    #[test]
    fn nested_function_definition_is_walked() {
        let src = r#"
def outer():
    def inner(x):
        if x > 0:
            if x < 10:
                return x
            elif x < 20:
                return x * 2
            elif x < 30:
                return x * 3
        return 0
    return inner
"#;
        let t = parse(src);
        let r = run(Path::new("a.py"), src, &t, &cfg());
        assert!(r.iter().any(|f| f.message.contains("`inner`")), "got: {:?}", r);
    }

    #[test]
    fn many_params_fire_but_self_is_free() {
        let many = "def f(a, b, c):\n    pass\n";
        let r = run(Path::new("a.py"), many, &parse(many), &cfg());
        assert!(r.iter().any(|f| f.detector == "python.params"), "got: {:?}", r);
        let method = "class C:\n    def f(self, a, b):\n        pass\n";
        let r = run(Path::new("a.py"), method, &parse(method), &cfg());
        assert!(r.iter().all(|f| f.detector != "python.params"), "got: {:?}", r);
    }
}
