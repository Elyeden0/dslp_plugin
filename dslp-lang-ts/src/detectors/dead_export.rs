//! `ts.dead-export`: exported declarations whose name is not referenced
//! anywhere else in the corpus.
//!
//! Heuristic and deliberately conservative — it matches on names only, and
//! counts *any* mention (imports, re-exports, local use, JSX, types) as a
//! reference. Skipped entirely for library packages (public API by design),
//! and for files a framework consumes by convention. Default exports and
//! `export { a }` clauses are not analysed.

use std::collections::HashSet;
use std::path::Path;

use dslp_core::{Finding, Severity, Span};
use dslp_lang_common::{span_of, walk};
use tree_sitter::{Node, Tree};

/// One `export <declaration>` found in a file.
pub struct ExportDecl {
    pub kind: &'static str,
    pub name: String,
    pub span: Span,
}

/// Directories whose files a framework loads by convention rather than by import.
const CONVENTION_DIRS: &[&str] = &[
    "pages",
    "app",
    "components",
    "composables",
    "plugins",
    "middleware",
    "server",
    "layouts",
    "utils",
];
const CONVENTION_STEMS: &[&str] = &[
    "index",
    "page",
    "layout",
    "route",
    "loading",
    "error",
    "not-found",
    "template",
    "middleware",
    "default",
    "main",
];

pub fn is_entry_or_convention(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let stem = name.split('.').next().unwrap_or("");
    let is_test = ["_test.", ".test.", ".spec."]
        .iter()
        .any(|m| name.contains(m))
        || path.components().any(|c| c.as_os_str() == "__tests__");
    name.ends_with(".d.ts")
        || name.contains(".config.")
        || is_test
        || CONVENTION_STEMS.contains(&stem)
        || path
            .components()
            .any(|c| CONVENTION_DIRS.contains(&c.as_os_str().to_str().unwrap_or("")))
}

/// Exports declared in `tree`, plus every identifier mention in it that is
/// not itself the name of one of those exports.
pub fn analyze(source: &str, tree: &Tree) -> (Vec<ExportDecl>, HashSet<String>) {
    let bytes = source.as_bytes();
    let mut decls = Vec::new();
    let mut own_names = HashSet::new();
    walk(tree.root_node(), &mut |node| {
        if node.kind() == "export_statement" && !is_default(node) {
            if let Some(decl) = node.child_by_field_name("declaration") {
                collect(decl, bytes, &mut decls, &mut own_names);
            }
        }
    });
    let mut refs = HashSet::new();
    walk(tree.root_node(), &mut |node| {
        let is_mention = matches!(
            node.kind(),
            "identifier"
                | "type_identifier"
                | "property_identifier"
                | "shorthand_property_identifier"
        );
        if is_mention && !own_names.contains(&node.id()) {
            if let Ok(text) = node.utf8_text(bytes) {
                refs.insert(text.to_string());
            }
        }
    });
    (decls, refs)
}

fn is_default(export: Node) -> bool {
    let mut cursor = export.walk();
    let found = export.children(&mut cursor).any(|c| c.kind() == "default");
    found
}

fn collect(decl: Node, bytes: &[u8], decls: &mut Vec<ExportDecl>, own_names: &mut HashSet<usize>) {
    let kind = match decl.kind() {
        "function_declaration" | "generator_function_declaration" => "function",
        "class_declaration" | "abstract_class_declaration" => "class",
        "interface_declaration" => "interface",
        "type_alias_declaration" => "type",
        "enum_declaration" => "enum",
        "lexical_declaration" => {
            let mut cursor = decl.walk();
            for var in decl
                .named_children(&mut cursor)
                .filter(|c| c.kind() == "variable_declarator")
            {
                if let Some(name) = var
                    .child_by_field_name("name")
                    .filter(|n| n.kind() == "identifier")
                {
                    push(name, "const", decl, bytes, decls, own_names);
                }
            }
            return;
        }
        _ => return,
    };
    if let Some(name) = decl.child_by_field_name("name") {
        push(name, kind, decl, bytes, decls, own_names);
    }
}

fn push(
    name_node: Node,
    kind: &'static str,
    decl: Node,
    bytes: &[u8],
    decls: &mut Vec<ExportDecl>,
    own_names: &mut HashSet<usize>,
) {
    if let Ok(name) = name_node.utf8_text(bytes) {
        own_names.insert(name_node.id());
        decls.push(ExportDecl {
            kind,
            name: name.to_string(),
            span: span_of(decl),
        });
    }
}

pub fn finding(path: &Path, decl: &ExportDecl) -> Finding {
    Finding::new(
        "ts.dead-export",
        Severity::Low,
        path.to_path_buf(),
        Some(decl.span),
        format!(
            "Exported {} `{}` isn't referenced anywhere else in the project.",
            decl.kind, decl.name
        ),
        Some(
            "Search for usages first (name matching only — dynamic imports and framework \
             conventions aren't tracked). If unused, delete it or drop the `export`."
                .into(),
        ),
        &format!("dead-export:{}:{}", decl.kind, decl.name),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(src: &str) -> (Vec<ExportDecl>, HashSet<String>) {
        analyze(src, &dslp_base::parse("typescript", src).unwrap())
    }

    #[test]
    fn collects_declaration_exports_but_not_default_or_clauses() {
        let (decls, _) = parsed(
            "export function a() {}\nexport const b = 1, c = 2;\nexport class D {}\nexport interface E {}\n\
             export type F = 1;\nexport enum G {}\nexport default function h() {}\nconst i = 1;\nexport { i };\n",
        );
        let names: Vec<_> = decls
            .iter()
            .map(|d| format!("{}:{}", d.kind, d.name))
            .collect();
        assert_eq!(
            names,
            [
                "function:a",
                "const:b",
                "const:c",
                "class:D",
                "interface:E",
                "type:F",
                "enum:G"
            ]
        );
    }

    #[test]
    fn own_declaration_name_is_not_a_reference_but_other_mentions_are() {
        let (_, refs) = parsed("export function a() {}\nexport function b() { return a(); }\n");
        assert!(refs.contains("a"), "a is called by b");
        assert!(!refs.contains("b"), "b is only declared");
    }

    #[test]
    fn convention_paths_are_recognised() {
        for p in [
            "src/index.ts",
            "app/foo/page.tsx",
            "pages/x.ts",
            "vite.config.ts",
            "src/a.test.ts",
            "types/a.d.ts",
        ] {
            assert!(is_entry_or_convention(Path::new(p)), "{p}");
        }
        assert!(!is_entry_or_convention(Path::new("src/lib/helpers.ts")));
    }
}
