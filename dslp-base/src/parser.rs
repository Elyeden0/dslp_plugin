use anyhow::{anyhow, Result};
use tree_sitter::{Language, Parser, Tree};

/// A registered grammar: its `lang_id` and a loader returning the tree-sitter
/// `Language`. Kept as a named type so the registry signature stays readable.
type GrammarEntry = (&'static str, fn() -> Language);

/// Parse `source` as `lang_id`. Currently `"rust"` only; the registry shape
/// keeps room for adding TS/Go/etc. later without changing the call site.
pub fn parse(lang_id: &str, source: &str) -> Result<Tree> {
    let language = language_for(lang_id)?;
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|e| anyhow!("failed to set tree-sitter language for {}: {}", lang_id, e))?;
    parser
        .parse(source, None)
        .ok_or_else(|| anyhow!("tree-sitter returned no tree for {}", lang_id))
}

fn language_for(lang_id: &str) -> Result<Language> {
    grammar_loaders()
        .iter()
        .find(|(id, _)| *id == lang_id)
        .map(|(_, load)| load())
        .ok_or_else(|| anyhow!("no tree-sitter grammar registered for {}", lang_id))
}

/// Per-grammar loader. Stays a flat data table so adding a language is a
/// one-line change and the cyclomatic count of the lookup function is trivial.
fn grammar_loaders() -> &'static [GrammarEntry] {
    &[
        ("rust", || tree_sitter_rust::language()),
        ("typescript", || {
            tree_sitter_typescript::language_typescript()
        }),
        ("tsx", || tree_sitter_typescript::language_tsx()),
        ("javascript", || tree_sitter_javascript::language()),
        ("go", || tree_sitter_go::language()),
        ("python", || tree_sitter_python::language()),
        ("c", || tree_sitter_c::language()),
        ("cpp", || tree_sitter_cpp::language()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_trivial_rust() {
        let tree = parse("rust", "fn main() {}").unwrap();
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(!root.has_error());
    }

    #[test]
    fn parses_trivial_typescript() {
        let tree = parse("typescript", "function main(): void {}").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_trivial_tsx() {
        let tree = parse("tsx", "const x = <div>hi</div>;").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_trivial_javascript() {
        let tree = parse("javascript", "function main() {}").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_trivial_go() {
        let tree = parse("go", "package main\n\nfunc main() {}\n").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_trivial_python() {
        let tree = parse("python", "def main():\n    return 0\n").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_trivial_c() {
        let tree = parse("c", "int main(void) { return 0; }").unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn parses_trivial_cpp() {
        let tree = parse(
            "cpp",
            "#include <vector>\nint main() { std::vector<int> v; return 0; }",
        )
        .unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn unknown_language_errors() {
        let err = parse("klingon", "qapla'").unwrap_err();
        assert!(err.to_string().contains("klingon"));
    }
}
