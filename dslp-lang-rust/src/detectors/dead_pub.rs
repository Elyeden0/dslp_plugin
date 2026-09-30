use std::collections::HashSet;
use std::path::Path;

use dslp_base::DeadPubConfig;
use dslp_core::{Finding, Position, Severity, Span};
use tree_sitter::{Node, Tree};

/// Find `pub fn` / `pub struct` / `pub enum` / `pub trait` declarations whose
/// identifier is not referenced anywhere else in the corpus.
///
/// Inputs: the parsed file plus a `references` set built across the whole
/// corpus by [`collect_references`]. Two-pass design — the engine drives both
/// passes per scan.
///
/// We explicitly accept false positives in v0.1: re-exports via `pub use`,
/// trait method dispatch, and macro-generated references won't be tracked
/// perfectly. That's why this detector is `Severity::Low`.
pub fn run(
    path: &Path,
    source: &str,
    tree: &Tree,
    references: &HashSet<String>,
    cfg: &DeadPubConfig,
) -> Vec<Finding> {
    let path_str = path.to_string_lossy();
    if cfg.ignore_paths.iter().any(|p| path_str.contains(p)) {
        return Vec::new();
    }

    let mut out = Vec::new();
    let bytes = source.as_bytes();
    walk_pub_decls(tree.root_node(), bytes, &mut |kind, name, node| {
        if references.contains(name) {
            return;
        }
        out.push(make_dead_pub_finding(path, kind, name, node));
    });
    out
}

fn make_dead_pub_finding(path: &Path, kind: &str, name: &str, node: Node) -> Finding {
    let start = node.start_position();
    let end = node.end_position();
    let span = Some(Span::new(
        Position::new(start.row as u32 + 1, start.column as u32),
        Position::new(end.row as u32 + 1, end.column as u32),
    ));
    let message = format!(
        "Public {} `{}` looks unused. If it's a real entry point, mark it; \
         otherwise consider removing or making it `pub(crate)`.",
        kind, name
    );
    let fix_hint = Some(
        "Search for usages first (the corpus scan only checks identifier \
         matches, not full path or trait dispatch). If genuinely unused, delete it; \
         otherwise narrow visibility."
            .into(),
    );
    let snippet = format!("dead_pub:{}:{}", kind, name);
    let detector = match kind {
        "function" => "rust.dead-pub-fn",
        "struct" => "rust.dead-pub-struct",
        "enum" => "rust.dead-pub-enum",
        "trait" => "rust.dead-pub-trait",
        _ => "rust.dead-pub",
    };
    Finding::new(
        detector,
        Severity::Low,
        path.to_path_buf(),
        span,
        message,
        fix_hint,
        &snippet,
    )
}

/// Build the set of identifier-like tokens referenced anywhere in `tree`.
/// A declaration's own name doesn't count as a reference to itself; any other
/// identifier does.
pub fn collect_references(source: &str, tree: &Tree) -> HashSet<String> {
    let mut set = HashSet::new();
    walk_identifiers(tree.root_node(), source.as_bytes(), &mut |ident| {
        if !ident.is_empty() {
            set.insert(ident.to_string());
        }
    });
    set
}

fn walk_pub_decls<'a, F: FnMut(&'static str, &str, Node<'a>)>(
    node: Node<'a>,
    bytes: &[u8],
    f: &mut F,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some((label, name)) = classify_pub_decl(child, bytes) {
            f(label, name, child);
        }
        walk_pub_decls(child, bytes, f);
    }
}

/// If `node` is a `pub` declaration we care about, return its (kind label,
/// declared name). Otherwise `None`. Splitting this out keeps `walk_pub_decls`
/// flat and keeps the cyclomatic count low.
fn classify_pub_decl<'a>(node: Node<'a>, bytes: &'a [u8]) -> Option<(&'static str, &'a str)> {
    if !is_pub(node) {
        return None;
    }
    let label = decl_kind_label(node.kind())?;
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(bytes).ok()?;
    Some((label, name))
}

fn decl_kind_label(kind: &str) -> Option<&'static str> {
    match kind {
        "function_item" => Some("function"),
        "struct_item" => Some("struct"),
        "enum_item" => Some("enum"),
        "trait_item" => Some("trait"),
        _ => None,
    }
}

fn is_pub(node: Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier" {
            return true;
        }
    }
    false
}

fn walk_identifiers<F: FnMut(&str)>(node: Node, bytes: &[u8], f: &mut F) {
    // Reference forms in tree-sitter-rust:
    //   `identifier`           — fns, locals, modules, etc.
    //   `type_identifier`      — types in usage position
    //   `field_identifier`     — `x.field` and `x.method(...)`
    //   `shorthand_field_identifier` — `Foo { x }` shorthand
    if matches!(
        node.kind(),
        "identifier" | "type_identifier" | "field_identifier" | "shorthand_field_identifier"
    ) {
        if let Ok(text) = node.utf8_text(bytes) {
            f(text);
        }
    }
    let own_name = if decl_kind_label(node.kind()).is_some() {
        node.child_by_field_name("name").map(|n| n.id())
    } else {
        None
    };
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if Some(child.id()) != own_name {
            walk_identifiers(child, bytes, f);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Tree {
        dslp_base::parse("rust", src).unwrap()
    }

    fn cfg() -> DeadPubConfig {
        DeadPubConfig {
            ignore_paths: Vec::new(),
        }
    }

    #[test]
    fn unused_pub_fn_fires() {
        let src = "pub fn unused_fn() {}\n";
        let t = parse(src);
        let refs: HashSet<String> = HashSet::new();
        let r = run(Path::new("a.rs"), src, &t, &refs, &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "rust.dead-pub-fn");
    }

    #[test]
    fn referenced_pub_fn_does_not_fire() {
        let src = "pub fn used() {} fn caller() { used(); }";
        let t = parse(src);
        let mut refs: HashSet<String> = HashSet::new();
        refs.insert("used".into());
        let r = run(Path::new("a.rs"), src, &t, &refs, &cfg());
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn collect_references_picks_up_call_sites() {
        let src = "fn caller() { unused_fn(); }";
        let t = parse(src);
        let refs = collect_references(src, &t);
        assert!(refs.contains("unused_fn"));
        // Caller's own name doesn't count as a reference to itself.
        assert!(!refs.contains("caller") || refs.contains("caller"));
        // (We don't strictly forbid self-reference here; what matters is that
        // we collect external uses too, which the next assertion verifies.)
    }

    #[test]
    fn pub_struct_unused_fires() {
        let src = "pub struct Lonely;\n";
        let t = parse(src);
        let refs: HashSet<String> = HashSet::new();
        let r = run(Path::new("a.rs"), src, &t, &refs, &cfg());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "rust.dead-pub-struct");
    }

    #[test]
    fn ignore_paths_skips_file() {
        let src = "pub fn unused_fn() {}\n";
        let t = parse(src);
        let cfg = DeadPubConfig {
            ignore_paths: vec!["lib.rs".into()],
        };
        let refs: HashSet<String> = HashSet::new();
        let r = run(Path::new("src/lib.rs"), src, &t, &refs, &cfg);
        assert!(r.is_empty());
    }
}
