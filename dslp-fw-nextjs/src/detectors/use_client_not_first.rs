//! `nextjs.use-client-not-first`: detects misplaced `'use client'` directives.
//!
//! Next.js requires the directive to be the first meaningful line — anything
//! before it (other than comments and an optional shebang on line 1) makes
//! Next silently treat the file as a server component. The bug is hard to
//! spot in review because the file LOOKS client-side.
//!
//! Detection is line-based, not AST-based, because we need to be tolerant of
//! files that don't parse cleanly. We walk lines from the top, skipping:
//!   - blank lines
//!   - line comments (`//...`)
//!   - block comments (`/* ... */` even multi-line)
//!   - a leading shebang on line 1
//!
//! When we hit the first non-skipped line:
//!   - If it's `'use client'` or `"use client"` (with optional `;` and
//!     trailing whitespace) → fine.
//!   - Otherwise, we scan the rest of the file for a `'use client'`
//!     line. If found, emit a finding pointing at it.

use std::path::Path;

use dslp_core::{Finding, Position, Severity, Span};

pub fn run(path: &Path, source: &str) -> Vec<Finding> {
    if !is_js_like(path) {
        return Vec::new();
    }
    let lines: Vec<&str> = source.lines().collect();
    let directive_idx = lines.iter().position(|l| is_use_client_directive(l));
    let Some(directive_idx) = directive_idx else {
        return Vec::new();
    };
    if Some(directive_idx) == first_meaningful_line(&lines) {
        return Vec::new();
    }
    vec![make_finding(path, &lines, directive_idx)]
}

fn is_js_like(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|e| matches!(e, "ts" | "tsx" | "js" | "jsx" | "cjs" | "mjs"))
        .unwrap_or(false)
}

fn make_finding(path: &Path, lines: &[&str], directive_idx: usize) -> Finding {
    let line_no = directive_idx as u32 + 1;
    let span = Some(Span::new(
        Position::new(line_no, 0),
        Position::new(line_no, lines[directive_idx].len() as u32),
    ));
    Finding::new(
        "nextjs.use-client-not-first",
        Severity::High,
        path.to_path_buf(),
        span,
        format!(
            "`'use client'` is on line {} but must be the first meaningful line of the module. \
             Next.js silently treats this file as a server component.",
            line_no
        ),
        Some(
            "Move the `'use client'` directive to the very top of the file (after only comments \
             and an optional shebang). Or, if the file is genuinely a server component, remove \
             the directive."
                .into(),
        ),
        "use-client-not-first",
    )
}

fn first_meaningful_line(lines: &[&str]) -> Option<usize> {
    let mut state = SkipState::Code;
    for (idx, line) in lines.iter().enumerate() {
        match step(&mut state, idx, line.trim()) {
            Step::Skip => continue,
            Step::Stop => return Some(idx),
        }
    }
    None
}

enum SkipState {
    Code,
    InBlockComment,
}

enum Step {
    Skip,
    Stop,
}

fn step(state: &mut SkipState, idx: usize, trimmed: &str) -> Step {
    match state {
        SkipState::InBlockComment => step_in_block_comment(state, trimmed),
        SkipState::Code => step_in_code(state, idx, trimmed),
    }
}

fn step_in_block_comment(state: &mut SkipState, trimmed: &str) -> Step {
    let Some((_, after)) = trimmed.split_once("*/") else {
        return Step::Skip;
    };
    *state = SkipState::Code;
    if has_meaningful_tail(after) {
        Step::Stop
    } else {
        Step::Skip
    }
}

fn step_in_code(state: &mut SkipState, idx: usize, trimmed: &str) -> Step {
    if is_blank_or_skippable_line(idx, trimmed) {
        return Step::Skip;
    }
    let Some(rest) = trimmed.strip_prefix("/*") else {
        return Step::Stop;
    };
    match rest.split_once("*/") {
        Some((_, after)) if has_meaningful_tail(after) => Step::Stop,
        Some(_) => Step::Skip,
        None => {
            *state = SkipState::InBlockComment;
            Step::Skip
        }
    }
}

fn is_blank_or_skippable_line(idx: usize, trimmed: &str) -> bool {
    trimmed.is_empty() || (idx == 0 && trimmed.starts_with("#!")) || trimmed.starts_with("//")
}

fn has_meaningful_tail(after: &str) -> bool {
    let after = after.trim();
    !after.is_empty() && !is_only_comment(after)
}

fn is_only_comment(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("//") || t.starts_with("/*") || t.is_empty()
}

fn is_use_client_directive(line: &str) -> bool {
    let t = line.trim();
    let t = t.trim_end_matches(';').trim_end();
    matches!(t, "'use client'" | "\"use client\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directive_first_line_does_not_fire() {
        let src = "'use client';\n\nimport React from 'react';\n";
        let r = run(Path::new("a.tsx"), src);
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn directive_after_imports_fires() {
        let src = "import React from 'react';\n\n'use client';\nexport default function App() { return null; }\n";
        let r = run(Path::new("a.tsx"), src);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].detector, "nextjs.use-client-not-first");
        assert_eq!(r[0].severity, Severity::High);
    }

    #[test]
    fn directive_after_block_comment_is_fine() {
        let src = "/*\n * A header comment.\n */\n'use client';\n\nexport default function App() { return null; }\n";
        let r = run(Path::new("a.tsx"), src);
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn directive_after_line_comments_is_fine() {
        let src = "// header\n// more header\n'use client';\nexport default function App() { return null; }\n";
        let r = run(Path::new("a.tsx"), src);
        assert!(r.is_empty(), "got: {:?}", r);
    }

    #[test]
    fn no_directive_is_silent() {
        let src = "import React from 'react';\nexport default function App() { return null; }\n";
        let r = run(Path::new("a.tsx"), src);
        assert!(r.is_empty());
    }

    #[test]
    fn directive_after_shebang_is_fine() {
        let src = "#!/usr/bin/env node\n'use client';\nexport {};\n";
        let r = run(Path::new("a.mjs"), src);
        assert!(r.is_empty());
    }

    #[test]
    fn double_quoted_directive_works() {
        let src = "import x from 'y';\n\"use client\";\n";
        let r = run(Path::new("a.tsx"), src);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn non_js_file_is_skipped() {
        let src = "'use client';\nrandom\nimport x from 'y';\n";
        let r = run(Path::new("a.rs"), src);
        assert!(r.is_empty());
    }
}
