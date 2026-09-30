//! Duplicate-block detector: runs of identical significant lines appearing in
//! more than one place, across every language at once.
//!
//! Works on normalised lines rather than syntax trees, so it needs no
//! per-language code. Blank lines, comments, imports and near-empty lines
//! (`}`, `);`, ...) are ignored so they can't create or hide duplicates. A
//! window of `min_lines` consecutive significant lines is hashed; windows
//! seen in two or more non-overlapping places are duplicates, and adjacent
//! duplicate windows merge into one reported range.
//!
//! Of N copies of a block, N-1 are reported (each pointing at the first
//! copy), so fixing one duplicate clears exactly one finding.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use dslp_base::Config;
use dslp_core::{FileHandle, Finding, LanguagePlugin, Position, ScanContext, Severity, Span};

const EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "cjs", "mjs", "go", "py", "pyi", "c", "h", "cpp", "cc", "cxx",
    "c++", "hpp", "hh", "hxx", "h++",
];

const IGNORED_PREFIXES: &[&str] = &[
    "//", "#", "/*", "*", "use ", "pub use ", "mod ", "import ", "from ", "package ", "export {",
    "export *",
];

/// Blocks at least this long (in significant lines) are Medium, not Low.
const MEDIUM_AT: usize = 20;

pub struct DupPlugin;

impl DupPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DupPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguagePlugin for DupPlugin {
    fn id(&self) -> &'static str {
        "duplicates"
    }

    fn extensions(&self) -> &'static [&'static str] {
        EXTENSIONS
    }

    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding> {
        let cfg: Config = serde_json::from_value(ctx.config.clone()).unwrap_or_default();
        find_duplicates(files, cfg.detectors.duplicates.min_lines.max(2))
    }
}

struct SigFile<'a> {
    path: &'a Path,
    /// (1-based line number, normalised text) for each significant line.
    lines: Vec<(u32, String)>,
}

fn significant_lines(source: &str) -> Vec<(u32, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, raw)| {
            let t = raw.trim();
            let too_plain = t.chars().filter(|c| c.is_alphanumeric()).count() < 3;
            if too_plain || IGNORED_PREFIXES.iter().any(|p| t.starts_with(p)) {
                return None;
            }
            Some((
                i as u32 + 1,
                t.split_whitespace().collect::<Vec<_>>().join(" "),
            ))
        })
        .collect()
}

type WindowKey = [u8; 16];

fn window_key(lines: &[(u32, String)]) -> WindowKey {
    let mut h = blake3::Hasher::new();
    for (_, text) in lines {
        h.update(text.as_bytes());
        h.update(b"\n");
    }
    h.finalize().as_bytes()[..16].try_into().expect("16 bytes")
}

fn hex(key: &WindowKey) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

/// Tests repeat setup on purpose; duplicated fixtures there aren't a smell.
fn is_test_path(path: &Path) -> bool {
    let in_test_dir = path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("tests" | "test" | "__tests__" | "spec")
        )
    });
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    in_test_dir
        || name.starts_with("test_")
        || ["_test.", ".test.", ".spec."]
            .iter()
            .any(|m| name.contains(m))
}

fn find_duplicates(files: &[FileHandle], window: usize) -> Vec<Finding> {
    let mut sorted: Vec<&FileHandle> = files.iter().filter(|f| !is_test_path(&f.path)).collect();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    let sig: Vec<SigFile> = sorted
        .iter()
        .map(|f| SigFile {
            path: &f.path,
            lines: significant_lines(&f.source),
        })
        .collect();

    // window hash -> every (file index, start index) it occurs at, in order.
    let mut seen: HashMap<WindowKey, Vec<(usize, usize)>> = HashMap::new();
    for (fi, f) in sig.iter().enumerate() {
        for start in 0..f.lines.len().saturating_sub(window - 1) {
            seen.entry(window_key(&f.lines[start..start + window]))
                .or_default()
                .push((fi, start));
        }
    }

    // Windows that have a non-overlapping twin elsewhere, per file.
    let mut dup_starts: Vec<BTreeMap<usize, WindowKey>> = vec![BTreeMap::new(); sig.len()];
    for (key, places) in &seen {
        for &(fi, start) in places {
            let has_twin = places
                .iter()
                .any(|&(fj, other)| fj != fi || other.abs_diff(start) >= window);
            if has_twin {
                dup_starts[fi].insert(start, *key);
            }
        }
    }

    let mut findings = Vec::new();
    for (fi, starts) in dup_starts.iter().enumerate() {
        for (run_start, run_end, key) in runs(starts) {
            let places = &seen[&key];
            let first = places.iter().min().copied().expect("non-empty");
            if first == (fi, run_start) {
                continue; // this is the original copy
            }
            findings.push(report(
                &sig,
                fi,
                run_start,
                run_end + window - 1,
                &key,
                first,
            ));
        }
    }
    findings
}

/// Merge consecutive duplicate window starts into (start, end, first key) runs.
fn runs(starts: &BTreeMap<usize, WindowKey>) -> Vec<(usize, usize, WindowKey)> {
    let mut out: Vec<(usize, usize, WindowKey)> = Vec::new();
    for (&s, &key) in starts {
        match out.last_mut() {
            Some(last) if last.1 + 1 == s => last.1 = s,
            _ => out.push((s, s, key)),
        }
    }
    out
}

fn report(
    sig: &[SigFile],
    fi: usize,
    start: usize,
    last: usize,
    key: &WindowKey,
    first: (usize, usize),
) -> Finding {
    let file = &sig[fi];
    let (from, to) = (file.lines[start].0, file.lines[last].0);
    let count = last - start + 1;
    let original = &sig[first.0];
    let message = format!(
        "{count} significant lines duplicate the block at {}:{}. Copy-pasted code drifts apart \
         over time.",
        original.path.display(),
        original.lines[first.1].0
    );
    let severity = if count >= MEDIUM_AT {
        Severity::Medium
    } else {
        Severity::Low
    };
    Finding::new(
        "dup.block",
        severity,
        file.path.to_path_buf(),
        Some(Span::new(Position::new(from, 0), Position::new(to, 0))),
        message,
        Some(
            "Extract the shared logic into one function/module and call it from both places."
                .into(),
        ),
        &hex(key),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn fh(path: &str, source: &str) -> FileHandle {
        FileHandle {
            path: PathBuf::from(path),
            source: Arc::<str>::from(source),
        }
    }

    fn block(prefix: &str, n: usize) -> String {
        (0..n)
            .map(|i| format!("    let {prefix}_{i} = compute_value({i});\n"))
            .collect()
    }

    #[test]
    fn copies_are_reported_once_pointing_at_the_original() {
        let shared: String = (0..10)
            .map(|i| format!("    total += weigh(item_{i});\n"))
            .collect();
        let a = format!("fn a() {{\n{shared}}}\n");
        let b = format!("fn b() {{\n    // unrelated\n{shared}}}\n");
        let found = find_duplicates(&[fh("z/b.rs", &b), fh("a/a.rs", &a)], 6);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].file, PathBuf::from("z/b.rs"));
        assert!(
            found[0].message.contains("a/a.rs:2"),
            "{}",
            found[0].message
        );
        assert!(found[0].message.starts_with("10 significant lines"));
    }

    #[test]
    fn short_or_trivial_overlap_is_ignored() {
        let five: String = (0..5)
            .map(|i| format!("    total += weigh(item_{i});\n"))
            .collect();
        assert!(find_duplicates(&[fh("a.rs", &five), fh("b.rs", &five)], 6).is_empty());
        let imports =
            "use a::b;\nuse c::d;\nuse e::f;\nuse g::h;\nuse i::j;\nuse k::l;\nuse m::n;\n";
        assert!(find_duplicates(&[fh("a.rs", imports), fh("b.rs", imports)], 3).is_empty());
        let braces = "}\n}\n});\n}\n}\n}\n}\n";
        assert!(find_duplicates(&[fh("a.rs", braces), fh("b.rs", braces)], 3).is_empty());
    }

    #[test]
    fn test_files_are_skipped() {
        let shared: String = (0..10)
            .map(|i| format!("    total += weigh(item_{i});\n"))
            .collect();
        for name in [
            "tests/a.rs",
            "src/a_test.go",
            "src/a.test.ts",
            "src/test_a.py",
            "web/__tests__/a.js",
        ] {
            assert!(
                find_duplicates(&[fh(name, &shared), fh("z.rs", &shared)], 6).is_empty(),
                "{name}"
            );
        }
    }

    #[test]
    fn distinct_code_is_not_flagged() {
        assert!(find_duplicates(
            &[fh("a.rs", &block("a", 12)), fh("b.rs", &block("b", 12))],
            6
        )
        .is_empty());
    }

    #[test]
    fn repeats_within_a_file_count_but_overlapping_windows_do_not() {
        let unit: String = (0..6)
            .map(|i| format!("    step_{i}(context);\n"))
            .collect();
        let once = fh("a.rs", &unit);
        assert!(find_duplicates(&[once], 6).is_empty());
        let twice = fh("b.rs", &format!("{unit}    separator_marker();\n{unit}"));
        assert_eq!(find_duplicates(&[twice], 6).len(), 1);
    }

    #[test]
    fn fingerprint_survives_line_shifts() {
        let shared: String = (0..8)
            .map(|i| format!("    total += weigh(item_{i});\n"))
            .collect();
        let a = fh("a.rs", &shared);
        let b1 = fh("b.rs", &shared);
        let b2 = fh("b.rs", &format!("\n\n\n{shared}"));
        let f1 = find_duplicates(&[a.clone(), b1], 6);
        let f2 = find_duplicates(&[a, b2], 6);
        assert_eq!(f1[0].fingerprint, f2[0].fingerprint);
        assert_ne!(f1[0].span, f2[0].span);
    }
}
