//! `nuxt.composable-naming`: TS/JS files inside a `composables/` directory
//! must export a function whose name starts with `use[A-Z|0-9]`. Nuxt 3's
//! auto-import relies on this convention — a file at `composables/foo.ts`
//! exporting `fetchUser` won't be auto-imported under the conventional name.
//!
//! Heuristic per file in `composables/`:
//!   - If the file has a default export, check its name. (Default-export
//!     identifiers are rarer in Nuxt composables; we don't penalize anonymous
//!     defaults like `export default function () {}` — they're a different
//!     anti-pattern out of scope here.)
//!   - Otherwise, check named function/const exports. If at least one
//!     exported function-like is well-named (`useXxx`), the file is fine.
//!     If ALL exported function-likes are mis-named, emit a finding.

use std::path::Path;

use dslp_core::{Finding, Position, Severity, Span};
use tree_sitter::{Node, Tree};

pub fn run(path: &Path, source: &str) -> Vec<Finding> {
    if !path_is_under_composables(path) {
        return Vec::new();
    }
    let lang = match path.extension().and_then(|s| s.to_str()) {
        Some("ts") => "typescript",
        Some("js") | Some("mjs") | Some("cjs") => "javascript",
        _ => return Vec::new(),
    };
    let Ok(tree) = dslp_base::parse(lang, source) else {
        return Vec::new();
    };
    let bytes = source.as_bytes();
    let names = collect_exported_function_names(&tree, bytes);
    if names.is_empty() {
        return Vec::new();
    }
    let any_well_named = names.iter().any(|(n, _)| is_composable_name(n));
    if any_well_named {
        return Vec::new();
    }
    // All exports are mis-named — emit a finding pointing at the first one.
    let (name, node_line) = &names[0];
    let span = Some(Span::new(
        Position::new(*node_line, 0),
        Position::new(*node_line, 0),
    ));
    vec![Finding::new(
        "nuxt.composable-naming",
        Severity::Medium,
        path.to_path_buf(),
        span,
        format!(
            "File `{}` is in `composables/` but its exported function `{}` doesn't follow Nuxt's \
             `use[A-Z]` convention. Auto-imports will not pick it up under the expected name.",
            path.display(),
            name
        ),
        Some(
            "Rename the exported function to start with `use` followed by an uppercase letter \
             (e.g. `useFoo`), or move the file out of `composables/` if it's not a composable."
                .into(),
        ),
        &format!("composable-naming:{}", name),
    )]
}

fn path_is_under_composables(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == "composables")
}

fn is_composable_name(name: &str) -> bool {
    let mut iter = name.chars();
    if iter.next() != Some('u') || iter.next() != Some('s') || iter.next() != Some('e') {
        return false;
    }
    matches!(iter.next(), Some(c) if c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Returns `(name, 1-indexed line)` for each function-like name that's
/// exported (named export, function declaration with `export`, or
/// `export const x = (...) => ...`).
fn collect_exported_function_names(tree: &Tree, bytes: &[u8]) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let root = tree.root_node();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "export_statement" {
            continue;
        }
        collect_from_export(child, bytes, &mut out);
    }
    out
}

fn collect_from_export(node: Node, bytes: &[u8], out: &mut Vec<(String, u32)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name) = child.child_by_field_name("name") {
                    if let Ok(text) = name.utf8_text(bytes) {
                        let line = child.start_position().row as u32 + 1;
                        out.push((text.to_string(), line));
                    }
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                collect_from_const(child, bytes, out);
            }
            _ => {}
        }
    }
}

fn collect_from_const(node: Node, bytes: &[u8], out: &mut Vec<(String, u32)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "variable_declarator" {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else {
            continue;
        };
        if !matches!(
            value.kind(),
            "arrow_function" | "function_expression" | "function"
        ) {
            continue;
        }
        let Some(name) = child.child_by_field_name("name") else {
            continue;
        };
        if let Ok(text) = name.utf8_text(bytes) {
            let line = child.start_position().row as u32 + 1;
            out.push((text.to_string(), line));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn well_named_composable_does_not_fire() {
        let src = "export function useUser() { return { name: 'a' }; }\n";
        let path = PathBuf::from("composables/useUser.ts");
        let r = run(&path, src);
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn misnamed_composable_fires() {
        let src = "export function fetchUser() { return { name: 'a' }; }\n";
        let path = PathBuf::from("composables/fetch.ts");
        let r = run(&path, src);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "nuxt.composable-naming");
        assert!(r[0].message.contains("fetchUser"));
    }

    #[test]
    fn arrow_const_misnamed_fires() {
        let src = "export const buildClient = () => ({});\n";
        let path = PathBuf::from("composables/buildClient.ts");
        let r = run(&path, src);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn misnamed_outside_composables_dir_does_not_fire() {
        let src = "export function fetchUser() { return { name: 'a' }; }\n";
        let path = PathBuf::from("utils/fetchUser.ts");
        let r = run(&path, src);
        assert!(r.is_empty());
    }

    #[test]
    fn one_good_export_among_many_does_not_fire() {
        let src = r#"
export function fetchUser() { return null; }
export function useThing() { return null; }
"#;
        let path = PathBuf::from("composables/mixed.ts");
        let r = run(&path, src);
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn no_function_exports_does_not_fire() {
        let src = "export const TIMEOUT = 5000;\nexport const URL = '...';\n";
        let path = PathBuf::from("composables/constants.ts");
        let r = run(&path, src);
        assert!(r.is_empty());
    }
}
