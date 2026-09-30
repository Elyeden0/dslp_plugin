//! Shared detector primitives for noAIslop language plugins.
//!
//! Every language plugin used to carry its own near-identical `count_lines`,
//! `cyclomatic`, `recur`, `length_lines`, `emit`, and file-size detector. A
//! knowledge-graph pass over the workspace surfaced the duplication; this
//! crate is the extraction. Per-language *differences* — which AST nodes are
//! functions, how names are spelled, which node kinds count as decision
//! points — stay in the plugins. The mechanical primitives live here.

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
    let mut score: u32 = 1;
    recur(node, decision_kinds, &mut score);
    score
}

fn recur(node: Node, decision_kinds: &[&str], score: &mut u32) {
    if decision_kinds.contains(&node.kind()) {
        *score += 1;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        recur(child, decision_kinds, score);
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

    #[test]
    fn count_lines_handles_trailing_newline() {
        assert_eq!(count_lines(""), 0);
        assert_eq!(count_lines("a\nb\n"), 2);
        assert_eq!(count_lines("a\nb"), 2);
    }

    #[test]
    fn cyclomatic_counts_decision_kinds() {
        let tree = dslp_base::parse("rust", "fn f(x: i32) -> i32 { if x > 0 { 1 } else { 2 } }").unwrap();
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
        assert!(file_size(std::path::Path::new("a.rs"), "a\nb\n", "rust.file-size", &cfg, &text).is_empty());
    }

    #[test]
    fn node_length_counts_inclusive_lines() {
        let src = "fn f() {\n  let x = 1;\n}\n";
        let tree = dslp_base::parse("rust", src).unwrap();
        let func = tree.root_node().child(0).unwrap();
        assert_eq!(node_length_lines(func), 3);
    }
}
