//! Comment-based slop detectors shared by every language, working on line
//! comments (`//` or `#`) so no parser is needed.
//!
//! - `<lang>.commented-code`: 3+ consecutive comment lines that read as code.
//! - `<lang>.obvious-comment`: a comment that only restates the line under it.

use std::collections::HashSet;
use std::path::Path;

use dslp_core::{Finding, Position, Severity, Span};

const MIN_CODE_RUN: usize = 3;
const KEYWORDS: &[&str] = &[
    "let ",
    "const ",
    "var ",
    "fn ",
    "def ",
    "class ",
    "return ",
    "if ",
    "for ",
    "while ",
    "elif ",
    "else ",
    "import ",
    "from ",
    "print(",
    "println!(",
    "func ",
    "match ",
    "try:",
    "except ",
];
const STOPWORDS: &[&str] = &[
    "the", "a", "an", "to", "of", "and", "for", "this", "that", "is", "are", "we", "it", "in",
    "on", "with", "by", "from", "then", "now", "here", "just", "new", "our", "if",
];
/// Comment markers that carry meaning for tools and are never commented code.
const DIRECTIVES: &[&str] = &[
    "noqa", "type:", "pylint", "fmt:", "isort", "pragma", "eslint", "nolint", "todo", "fixme",
    "safety",
];

struct Line<'a> {
    no: u32,
    raw: &'a str,
    /// Text after the comment marker, if this is a plain line comment.
    comment: Option<&'a str>,
}

fn classify<'a>(no: u32, raw: &'a str, marker: &str) -> Line<'a> {
    let t = raw.trim();
    let comment = t.strip_prefix(marker).filter(|rest| {
        let doc = marker == "//" && (rest.starts_with('/') || rest.starts_with('!'));
        let shebang = marker == "#" && rest.starts_with('!');
        !doc && !shebang
    });
    Line {
        no,
        raw,
        comment: comment.map(str::trim),
    }
}

fn looks_like_code(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    if text.is_empty() || DIRECTIVES.iter().any(|d| lower.contains(d)) {
        return false;
    }
    let ends_like_code = text.ends_with(';') || text.ends_with('{') || text.ends_with('}');
    let keyword_with_syntax = KEYWORDS.iter().any(|k| text.starts_with(k))
        && text
            .chars()
            .any(|c| matches!(c, '(' | ')' | '{' | '}' | ';' | '=' | ':'));
    let is_call = text.ends_with(')') || text.ends_with(");");
    let starts_ident = text
        .chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_');
    let call = is_call
        && starts_ident
        && text.contains('(')
        && !text.contains(". ")
        && !text.contains(' ');
    let assign = starts_ident && is_assignment(text);
    ends_like_code || keyword_with_syntax || call || assign
}

fn is_assignment(text: &str) -> bool {
    let Some((lhs, rhs)) = text.split_once(" = ") else {
        return false;
    };
    !lhs.contains(' ')
        && !rhs.trim().is_empty()
        && lhs
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '[' | ']'))
}

/// What a comment detector found, before it becomes a [`Finding`].
struct Hit<'a> {
    id: &'a str,
    from: u32,
    to: u32,
    message: String,
    hint: &'a str,
    key: String,
}

impl Hit<'_> {
    fn into_finding(self, path: &Path, lang: &str) -> Finding {
        Finding::new(
            format!("{lang}.{}", self.id),
            Severity::Low,
            path.to_path_buf(),
            Some(Span::new(
                Position::new(self.from, 0),
                Position::new(self.to, 0),
            )),
            self.message,
            Some(self.hint.to_string()),
            &self.key,
        )
    }
}

pub fn run(path: &Path, source: &str, lang: &str, marker: &str) -> Vec<Finding> {
    let lines: Vec<Line> = source
        .lines()
        .enumerate()
        .map(|(i, l)| classify(i as u32 + 1, l, marker))
        .collect();
    let mut out = Vec::new();
    let mut in_code_run = vec![false; lines.len()];

    let mut i = 0;
    while i < lines.len() {
        let mut j = i;
        while j < lines.len() && lines[j].comment.is_some_and(looks_like_code) {
            j += 1;
        }
        if j - i >= MIN_CODE_RUN {
            in_code_run[i..j].iter_mut().for_each(|b| *b = true);
            let first = lines[i].comment.unwrap_or_default();
            out.push(
                Hit {
                    id: "commented-code",
                    from: lines[i].no,
                    to: lines[j - 1].no,
                    message: format!("{} consecutive comment lines look like commented-out code.", j - i),
                    hint: "Delete it — version control already remembers old code. If it documents an alternative, describe it in prose.",
                    key: format!("commented-code:{first}"),
                }
                .into_finding(path, lang),
            );
        }
        i = j.max(i + 1);
    }

    for (idx, pair) in lines.windows(2).enumerate() {
        let (c, next) = (&pair[0], &pair[1]);
        let Some(text) = c.comment else { continue };
        if in_code_run[idx] || next.comment.is_some() || next.raw.trim().is_empty() {
            continue;
        }
        if restates(text, next.raw) {
            out.push(
                Hit {
                    id: "obvious-comment",
                    from: c.no,
                    to: c.no,
                    message: format!("Comment `{text}` only restates the code below it."),
                    hint: "Delete it, or say *why* the code does this rather than *what* it does.",
                    key: format!("obvious-comment:{}", next.raw.trim()),
                }
                .into_finding(path, lang),
            );
        }
    }
    out
}

fn stem(word: &str) -> &str {
    word.strip_suffix('s')
        .filter(|w| w.len() > 2)
        .unwrap_or(word)
}

/// Lowercased words of an identifier-ish token, split on `_` and camelCase.
fn identifier_parts(code: &str) -> HashSet<String> {
    let mut parts = HashSet::new();
    for token in code.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let mut cur = String::new();
        let mut flush = |cur: &mut String| {
            if !cur.is_empty() {
                parts.insert(stem(&cur.to_lowercase()).to_string());
                cur.clear();
            }
        };
        let mut prev_lower = false;
        for c in token.chars() {
            if c == '_' || (c.is_uppercase() && prev_lower) {
                flush(&mut cur);
            }
            if c != '_' {
                cur.push(c);
            }
            prev_lower = c.is_lowercase();
        }
        flush(&mut cur);
    }
    parts
}

fn restates(comment: &str, code: &str) -> bool {
    let words: Vec<String> = comment
        .split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|w| w.len() > 1 && !STOPWORDS.contains(&w.as_str()))
        .map(|w| stem(&w).to_string())
        .collect();
    if !(2..=5).contains(&words.len()) || comment.contains(['?', ':']) {
        return false;
    }
    let parts = identifier_parts(code);
    words.iter().all(|w| parts.contains(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(src: &str, marker: &str) -> Vec<String> {
        run(Path::new("f"), src, "x", marker)
            .into_iter()
            .map(|f| f.detector)
            .collect()
    }

    #[test]
    fn commented_out_code_needs_a_run_of_code_like_lines() {
        let code =
            "fn a() {}\n// let x = compute(1);\n// if x > 0 {\n//     return x;\n// }\nfn b() {}\n";
        assert_eq!(ids(code, "//"), ["x.commented-code"]);
        let two = "// let x = compute(1);\n// return x;\nfn b() {}\n";
        assert!(ids(two, "//").is_empty());
        let prose = "// This function computes the total.\n// It walks every item once.\n// Then it returns the sum.\nfn f() {}\n";
        assert!(ids(prose, "//").is_empty());
    }

    #[test]
    fn python_hash_comments_and_directives() {
        let py = "# x = load(1)\n# y = load(2)\n# print(x, y)\nz = 1\n";
        assert_eq!(ids(py, "#"), ["x.commented-code"]);
        let directives =
            "#!/usr/bin/env python\n# type: ignore\n# noqa: E501\n# pylint: disable=x\nz = 1\n";
        assert!(ids(directives, "#").is_empty());
    }

    #[test]
    fn doc_comments_are_never_flagged() {
        let doc = "/// let x = 1;\n/// let y = 2;\n/// let z = 3;\nfn f() {}\n";
        assert!(ids(doc, "//").is_empty());
    }

    #[test]
    fn obvious_comments_are_flagged_but_explanations_are_not() {
        assert_eq!(
            ids("// return the result\nreturn result;\n", "//"),
            ["x.obvious-comment"]
        );
        assert_eq!(
            ids("// create user record\nlet user_record = create();\n", "//"),
            ["x.obvious-comment"]
        );
        assert!(ids(
            "// retry because the API rate-limits bursts\nretry(call);\n",
            "//"
        )
        .is_empty());
        assert!(
            ids("// increment\ncounter += 1;\n", "//").is_empty(),
            "single word is too weak a signal"
        );
    }

    #[test]
    fn fingerprints_ignore_line_shifts() {
        let a = run(
            Path::new("f"),
            "// return the result\nreturn result;\n",
            "x",
            "//",
        );
        let b = run(
            Path::new("f"),
            "\n\n// return the result\nreturn result;\n",
            "x",
            "//",
        );
        assert_eq!(a[0].fingerprint, b[0].fingerprint);
    }
}
