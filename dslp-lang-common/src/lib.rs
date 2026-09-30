//! Shared detector primitives for noAIslop language plugins.
//!
//! Every language plugin used to carry its own near-identical `count_lines`,
//! `cyclomatic`, `recur`, `length_lines`, `emit`, and file-size detector. A
//! knowledge-graph pass over the workspace surfaced the duplication; this
//! crate is the extraction. Per-language *differences* — which AST nodes are
//! functions, how names are spelled, which node kinds count as decision
//! points — stay in the plugins. The mechanical primitives live here.

pub mod comments;

use std::path::Path;

use dslp_base::FileSizeConfig;
use dslp_core::{Finding, Position, Severity, Span};
use tree_sitter::Node;

/// Count source lines. A final line without a trailing newline still counts.
pub fn count_lines(source: &str) -> u32 {
    if source.is_empty() {
        return 0;
    }
    let nl = source.bytes().filter(|b| *b == b'\n').count() as u32;
    if source.ends_with('\n') {
        nl
    } else {
        nl + 1
    }
}

/// 1-indexed-line span covering a tree-sitter node.
pub fn span_of(node: Node) -> Span {
    let s = node.start_position();
    let e = node.end_position();
    Span::new(
        Position::new(s.row as u32 + 1, s.column as u32),
        Position::new(e.row as u32 + 1, e.column as u32),
    )
}

/// Line count a node spans (inclusive of both end lines).
pub fn node_length_lines(node: Node) -> u32 {
    let s = node.start_position().row as u32;
    let e = node.end_position().row as u32;
    e.saturating_sub(s) + 1
}

/// Cyclomatic complexity over a subtree: 1 + the number of descendant nodes
/// whose `kind()` is in `decision_kinds`. The kind set is the only
/// per-language difference, so callers pass their own.
pub fn cyclomatic(node: Node, decision_kinds: &[&str]) -> u32 {
    cyclomatic_excluding(node, decision_kinds, &[])
}

/// Like [`cyclomatic`], but does not descend into nodes whose kind is in
/// `skip_kinds` (nested named functions, which are scored on their own and
/// shouldn't also inflate the enclosing function).
pub fn cyclomatic_excluding(node: Node, decision_kinds: &[&str], skip_kinds: &[&str]) -> u32 {
    let mut score: u32 = 1;
    recur(node, decision_kinds, skip_kinds, &mut score);
    score
}

fn recur(node: Node, decision_kinds: &[&str], skip_kinds: &[&str], score: &mut u32) {
    if decision_kinds.contains(&node.kind()) {
        *score += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if !skip_kinds.contains(&child.kind()) {
            recur(child, decision_kinds, skip_kinds, score);
        }
    }
}

/// Build a Finding anchored at a node's span.
pub fn finding_at_node(
    path: &Path,
    node: Node,
    detector: &'static str,
    severity: Severity,
    message: String,
    fix_hint: &str,
    snippet: &str,
) -> Finding {
    Finding::new(
        detector,
        severity,
        path.to_path_buf(),
        Some(span_of(node)),
        message,
        Some(fix_hint.to_string()),
        snippet,
    )
}

/// Pre-order visit of every node in a subtree.
pub fn walk<'a>(node: Node<'a>, f: &mut impl FnMut(Node<'a>)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk(child, f);
    }
}

/// Name of the nearest enclosing node whose kind is in `fn_kinds`, or
/// `<top-level>`. Used to key slop findings on *where* they are rather than
/// on a line number, so their fingerprints survive edits above them.
pub fn enclosing_name(node: Node, bytes: &[u8], fn_kinds: &[&str]) -> String {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if fn_kinds.contains(&n.kind()) {
            let name = n
                .child_by_field_name("name")
                .and_then(|x| x.utf8_text(bytes).ok());
            return name.unwrap_or("<anonymous>").to_string();
        }
        cur = n.parent();
    }
    "<top-level>".to_string()
}

/// A finding anchored at `node`, keyed by detector + enclosing function.
#[allow(clippy::too_many_arguments)]
pub fn item_finding(
    path: &Path,
    node: Node,
    bytes: &[u8],
    fn_kinds: &[&str],
    detector: &'static str,
    severity: Severity,
    message: &str,
    fix_hint: &str,
) -> Finding {
    let scope = enclosing_name(node, bytes, fn_kinds);
    finding_at_node(
        path,
        node,
        detector,
        severity,
        format!("{message} (in `{scope}`)"),
        fix_hint,
        &format!("{detector}:{scope}"),
    )
}

/// Deepest chain of nested `kinds` nodes under `node`. An `else if` (a kind
/// node directly inside one of `else_parents`) continues its chain instead of
/// nesting deeper.
pub fn max_nesting(node: Node, kinds: &[&str], else_parents: &[&str]) -> u32 {
    let is_kind = kinds.contains(&node.kind());
    let chained = is_kind
        && node
            .parent()
            .is_some_and(|p| else_parents.contains(&p.kind()));
    let own = u32::from(is_kind && !chained);
    let mut cursor = node.walk();
    let deepest = node
        .children(&mut cursor)
        .map(|c| max_nesting(c, kinds, else_parents))
        .max()
        .unwrap_or(0);
    own + deepest
}

/// A Low-severity "deeply nested" finding when `depth >= threshold`.
pub fn nesting_finding(
    path: &Path,
    fn_node: Node,
    detector: &'static str,
    name: &str,
    depth: u32,
    threshold: u32,
) -> Option<Finding> {
    (depth >= threshold).then(|| {
        finding_at_node(
            path,
            fn_node,
            detector,
            Severity::Low,
            format!(
                "Function `{}` nests control flow {} levels deep (threshold {}).",
                name, depth, threshold
            ),
            "Flatten with early returns/guard clauses, or extract the inner block into its own function.",
            &format!("{detector}:{name}"),
        )
    })
}

/// Declared parameters in a `parameters` node, ignoring comments and Rust's
/// `self` receiver.
pub fn param_count(params: Node) -> u32 {
    let mut cursor = params.walk();
    params
        .named_children(&mut cursor)
        .filter(|n| !matches!(n.kind(), "comment" | "self_parameter"))
        .count() as u32
}

/// A Low-severity "too many parameters" finding when `count >= threshold`.
pub fn params_finding(
    path: &Path,
    fn_node: Node,
    detector: &'static str,
    name: &str,
    count: u32,
    threshold: u32,
) -> Option<Finding> {
    (count >= threshold).then(|| {
        finding_at_node(
            path,
            fn_node,
            detector,
            Severity::Low,
            format!(
                "Function `{}` takes {} parameters (threshold {}). Long parameter lists \
                 usually mean a missing type.",
                name, count, threshold
            ),
            "Group related parameters into a struct/object/dataclass, or split the \
             function so each part needs fewer inputs.",
            &format!("{detector}:{name}"),
        )
    })
}

/// Per-language wording for the generic file-size detector.
pub struct FileSizeText {
    /// Appended after "File has N lines (threshold: T). " — e.g.
    /// "Consider splitting along module boundaries."
    pub message_tail: &'static str,
    pub fix_hint: &'static str,
}

/// The file-size detector, shared by every language. Emits one Finding when
/// the file is at/over `cfg.threshold`, escalating to High at `cfg.severity_at`.
pub fn file_size(
    path: &Path,
    source: &str,
    detector_id: &'static str,
    cfg: &FileSizeConfig,
    text: &FileSizeText,
) -> Vec<Finding> {
    let n = count_lines(source);
    if n < cfg.threshold {
        return Vec::new();
    }
    let severity = if n >= cfg.severity_at {
        Severity::High
    } else {
        Severity::Medium
    };
    let span = Some(Span::new(Position::new(1, 0), Position::new(n, 0)));
    let message = format!(
        "File has {} lines (threshold: {}). {}",
        n, cfg.threshold, text.message_tail
    );
    vec![Finding::new(
        detector_id,
        severity,
        path.to_path_buf(),
        span,
        message,
        Some(text.fix_hint.to_string()),
        "file_size",
    )]
}

/// Name of a C/C++ `function_definition`, seen through pointer, reference,
/// parenthesized and qualified declarators (`Foo::bar`, operators, destructors).
pub fn c_family_function_name(fn_node: Node, bytes: &[u8]) -> Option<String> {
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
    matches!(
        kind,
        "qualified_identifier" | "operator_name" | "destructor_name"
    )
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

    #[test]
    fn count_lines_handles_trailing_newline() {
        assert_eq!(count_lines(""), 0);
        assert_eq!(count_lines("a\nb\n"), 2);
        assert_eq!(count_lines("a\nb"), 2);
    }

    #[test]
    fn cyclomatic_counts_decision_kinds() {
        let tree =
            dslp_base::parse("rust", "fn f(x: i32) -> i32 { if x > 0 { 1 } else { 2 } }").unwrap();
        // one if_expression → base 1 + 1 = 2
        let n = cyclomatic(tree.root_node(), &["if_expression"]);
        assert_eq!(n, 2);
    }

    #[test]
    fn cyclomatic_base_is_one_with_no_decisions() {
        let tree = dslp_base::parse("rust", "fn f() {}").unwrap();
        assert_eq!(cyclomatic(tree.root_node(), &["if_expression"]), 1);
    }

    #[test]
    fn file_size_fires_above_threshold_with_tail() {
        let cfg = FileSizeConfig {
            threshold: 3,
            severity_at: 10,
        };
        let text = FileSizeText {
            message_tail: "Split it.",
            fix_hint: "Extract a module.",
        };
        let r = file_size(
            std::path::Path::new("a.rs"),
            "1\n2\n3\n",
            "rust.file-size",
            &cfg,
            &text,
        );
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "rust.file-size");
        assert!(r[0].message.contains("Split it."));
        assert_eq!(r[0].severity, Severity::Medium);
    }

    #[test]
    fn file_size_silent_below_threshold() {
        let cfg = FileSizeConfig {
            threshold: 100,
            severity_at: 200,
        };
        let text = FileSizeText {
            message_tail: "x",
            fix_hint: "y",
        };
        assert!(file_size(
            std::path::Path::new("a.rs"),
            "a\nb\n",
            "rust.file-size",
            &cfg,
            &text
        )
        .is_empty());
    }

    #[test]
    fn node_length_counts_inclusive_lines() {
        let src = "fn f() {\n  let x = 1;\n}\n";
        let tree = dslp_base::parse("rust", src).unwrap();
        let func = tree.root_node().child(0).unwrap();
        assert_eq!(node_length_lines(func), 3);
    }
}
