//! `react.hook-return-bloat`: custom hooks (`useXxx`) that return an object
//! literal with too many fields are "doing too much" — a refactor smell.
//!
//! Threshold: 12 fields, matching desloppify's reference detector.
//!
//! Approach: walk function declarations / arrow functions whose name starts
//! with `use[A-Z]`. For each, find the *last* top-level `return` whose value
//! is an object literal, count its top-level properties, and emit a finding
//! if the count exceeds the threshold.

use std::path::Path;

use dslp_core::{Finding, Position, Severity, Span};
use tree_sitter::{Node, Tree};

const FIELD_THRESHOLD: usize = 12;

pub fn run(path: &Path, source: &str, tree: &Tree) -> Vec<Finding> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    walk_hooks(tree.root_node(), bytes, &mut |fn_node, name| {
        let body = fn_node.child_by_field_name("body");
        let Some(body) = body else { return };
        let Some(field_count) = last_return_object_field_count(body) else {
            return;
        };
        if field_count <= FIELD_THRESHOLD {
            return;
        }
        let start = fn_node.start_position();
        let end = fn_node.end_position();
        let span = Some(Span::new(
            Position::new(start.row as u32 + 1, start.column as u32),
            Position::new(end.row as u32 + 1, end.column as u32),
        ));
        out.push(Finding::new(
            "react.hook-return-bloat",
            Severity::Medium,
            path.to_path_buf(),
            span,
            format!(
                "Custom hook `{}` returns an object with {} fields (threshold {}). \
                 Hooks returning many fields tend to bundle unrelated concerns.",
                name, field_count, FIELD_THRESHOLD
            ),
            Some(
                "Split into multiple smaller hooks (e.g., one per concern) and let \
                 callers compose them, or expose a smaller interface and keep the \
                 rest internal."
                    .into(),
            ),
            &format!("hook-return-bloat:{}", name),
        ));
    });
    out
}

fn walk_hooks<'a, F: FnMut(Node<'a>, &str)>(node: Node<'a>, bytes: &[u8], f: &mut F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = hook_name(child, bytes) {
            f(child, &name);
        }
        walk_hooks(child, bytes, f);
    }
}

/// If `node` is a custom-hook function (named `useXxx` where the next char is
/// uppercase or a digit), return the name. Handles:
///   `function useFoo() {}`
///   `const useFoo = () => {}`     (arrow fn with parent variable_declarator)
///   `export const useFoo = ...`
fn hook_name(node: Node, bytes: &[u8]) -> Option<String> {
    let raw = function_or_arrow_name(node, bytes)?;
    if is_custom_hook_name(&raw) {
        Some(raw)
    } else {
        None
    }
}

/// Extract the declared name from a function declaration, function expression,
/// or arrow function (the latter via its parent `variable_declarator`).
/// Returns `None` if the node is not a function-like or has no name.
fn function_or_arrow_name(node: Node, bytes: &[u8]) -> Option<String> {
    match node.kind() {
        "function_declaration" | "function" => name_field(node, bytes),
        "arrow_function" | "function_expression" => {
            let parent = node.parent()?;
            if parent.kind() != "variable_declarator" {
                return None;
            }
            name_field(parent, bytes)
        }
        _ => None,
    }
}

fn name_field(node: Node, bytes: &[u8]) -> Option<String> {
    Some(
        node.child_by_field_name("name")?
            .utf8_text(bytes)
            .ok()?
            .to_string(),
    )
}

fn is_custom_hook_name(s: &str) -> bool {
    let mut iter = s.chars();
    if iter.next() != Some('u') || iter.next() != Some('s') || iter.next() != Some('e') {
        return false;
    }
    let next = iter.next();
    matches!(next, Some(c) if c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// In `body`, find the *last* `return` statement at the body's top level
/// whose argument is an object literal, and count its top-level properties.
fn last_return_object_field_count(body: Node) -> Option<usize> {
    let mut last: Option<Node> = None;
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "return_statement" {
            // Make sure this return is at the body's top level — not inside a
            // nested fn — by checking its parent is the body. tree-sitter's
            // walking already ensures we only see direct children, so this is
            // satisfied.
            last = Some(child);
        }
    }
    let return_stmt = last?;
    let arg = return_stmt.named_children(&mut return_stmt.walk()).next()?;
    if arg.kind() != "object" {
        return None;
    }
    let mut count = 0usize;
    let mut cursor = arg.walk();
    for child in arg.named_children(&mut cursor) {
        match child.kind() {
            "pair"
            | "method_definition"
            | "shorthand_property_identifier"
            | "shorthand_property_identifier_pattern"
            | "spread_element"
            | "computed_property_name" => count += 1,
            _ => {}
        }
    }
    Some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_tsx(src: &str) -> Tree {
        dslp_base::parse("tsx", src).unwrap()
    }

    #[test]
    fn hook_with_few_fields_does_not_fire() {
        let src = r#"
function useUser() {
    return { name: "alex", age: 22, role: "engineer" };
}
"#;
        let t = parse_tsx(src);
        let r = run(Path::new("a.tsx"), src, &t);
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn hook_with_too_many_fields_fires() {
        let body: String = (0..15)
            .map(|i| format!("        f{}: {}", i, i))
            .collect::<Vec<_>>()
            .join(",\n");
        let src = format!(
            "function useFat() {{\n    return {{\n{}\n    }};\n}}\n",
            body
        );
        let t = parse_tsx(&src);
        let r = run(Path::new("a.tsx"), &src, &t);
        assert_eq!(r.len(), 1, "got: {:?}", r);
        assert_eq!(r[0].detector, "react.hook-return-bloat");
        assert!(r[0].message.contains("`useFat`"));
    }

    #[test]
    fn arrow_hook_via_const_assignment_works() {
        let body: String = (0..15)
            .map(|i| format!("        f{}: {}", i, i))
            .collect::<Vec<_>>()
            .join(",\n");
        let src = format!(
            "const useArrow = () => {{\n    return {{\n{}\n    }};\n}};\n",
            body
        );
        let t = parse_tsx(&src);
        let r = run(Path::new("a.tsx"), &src, &t);
        assert_eq!(r.len(), 1);
        assert!(r[0].message.contains("`useArrow`"));
    }

    #[test]
    fn non_hook_function_with_huge_return_does_not_fire() {
        let body: String = (0..15)
            .map(|i| format!("        f{}: {}", i, i))
            .collect::<Vec<_>>()
            .join(",\n");
        let src = format!(
            "function buildConfig() {{\n    return {{\n{}\n    }};\n}}\n",
            body
        );
        let t = parse_tsx(&src);
        let r = run(Path::new("a.tsx"), &src, &t);
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn use_lowercase_is_not_a_hook() {
        // `useragent` starts with `use` but next char is lowercase.
        let body: String = (0..15)
            .map(|i| format!("        f{}: {}", i, i))
            .collect::<Vec<_>>()
            .join(",\n");
        let src = format!(
            "function useragent() {{\n    return {{\n{}\n    }};\n}}\n",
            body
        );
        let t = parse_tsx(&src);
        let r = run(Path::new("a.tsx"), &src, &t);
        assert!(r.is_empty(), "got: {:?}", r);
    }
}
