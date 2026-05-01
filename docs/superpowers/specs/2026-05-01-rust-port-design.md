# noAIslop — Rust port of desloppify (design)

**Status:** v0.1 spec, brainstormed 2026-05-01
**Reference implementation:** [peteromallet/desloppify](https://github.com/peteromallet/desloppify) (Python, ~258k LoC)
**Author:** Alexis Bonnard + Claude (autonomous brainstorm/build session)

## Context and goal

`noAIslop` is a Rust reimplementation of the analyse-codebase → score → "what to fix" core of desloppify. It is **not a faithful one-to-one port**: same architectural shape, our own detectors and scoring formula. We can grow toward full fidelity later if we want; we are not locking ourselves out.

### What's in scope (v0.1, the slice that ships first)

- A binary, `dslp`, that scans a Rust codebase and produces a prioritized list of issues to fix
- One language: **Rust** (we dogfood on this very repo)
- Three detectors, deliberately small to validate the framework:
  1. **File size / module bloat** — flag files over a configurable line threshold
  2. **Function complexity** — cyclomatic + length heuristic
  3. **Dead public items** — `pub fn`/`pub struct`/`pub enum` with no in-crate references and no `#[test]` / `#[cfg(test)]` consumer
- Scoring: a single composite score 0-100, no anti-gaming yet, no lenient/strict split
- Persistent state in `.dslp/` (JSON), so `next` returns deterministic ordering across invocations
- CLI: `dslp scan` and `dslp next`

### What's explicitly out of scope for v0.1

- Languages other than Rust (added one slice at a time)
- Framework awareness (React/Next.js/Nuxt — added after generic TS/JS lands)
- LLM "intelligence" review of subjective dimensions
- Anti-gaming machinery (wontfix penalties, lenient/strict split)
- Plan/triage workflow (`plan`, `plan queue`, `backlog`, `resolve`, `suppress`)
- Scorecard badge generation
- Autofix
- Cross-language monorepo support (`--lang` flag is reserved but unused in v0.1)

### Order of follow-on slices

1. v0.1: Rust language plugin (this spec)
2. v0.2: TS/JS plugin (so the user can test on web projects)
3. v0.3: framework spec abstraction + React detectors
4. v0.4: Next.js spec
5. v0.5: Nuxt spec (net-new — desloppify has no Nuxt support to crib from)
6. then: Go, Python, C, C++ in that order

---

## Section 1 — Architecture (approved)

Cargo workspace, six crates:

| Crate | Responsibility | Depends on |
|---|---|---|
| `dslp-core` | Domain types: `Issue`, `Finding`, `Score`, `Queue`, plugin traits. Pure, no I/O, no parser deps. | — |
| `dslp-base` | Infra: config, file discovery (gitignore + user excludes), tree-sitter glue, output formatting. | `dslp-core` |
| `dslp-engine` | The scan → score → queue pipeline. Calls plugins via traits. Owns persistent state. | `dslp-core`, `dslp-base` |
| `dslp-lang-rust` | Rust language plugin. (Later: `dslp-lang-ts`, `-go`, …) | `dslp-core`, `dslp-base` |
| `dslp-fw-react` | (Future) React framework spec. Plugs into TS/JS plugin. | `dslp-core`, `dslp-base` |
| `dslp-cli` | The binary. Parses args, calls engine, formats output. Statically registers all plugins. | All of the above |

Three load-bearing decisions:

- **Plugins are statically linked, not dyn libraries.** Adding a language = add a crate, register it in `dslp-cli`. No dyn-load dance, no ABI churn. Trade runtime extensibility for simplicity.
- **`core` has zero parser/tree-sitter deps.** Only `base` and language plugins know about parsing. Scoring/queue logic stays unit-testable without code corpora.
- **State lives in `engine`, plugins are stateless.** A plugin is `scan(files) -> Vec<Finding>`. The engine accumulates persistent state across runs. Avoids "plugin cache vs engine state out of sync" bugs.

---

## Section 2 — Plugin contract & detector model

### `LanguagePlugin` trait (in `dslp-core`)

```rust
pub trait LanguagePlugin: Send + Sync {
    /// Stable id, e.g. "rust", "typescript".
    fn id(&self) -> &'static str;

    /// File extensions this plugin claims (without the dot).
    fn extensions(&self) -> &'static [&'static str];

    /// Run all of this plugin's detectors against the given file set.
    /// Returns a flat list of findings; the engine handles dedup, ranking, scoring.
    fn scan(&self, ctx: &ScanContext, files: &[FileHandle]) -> Vec<Finding>;
}
```

`ScanContext` carries: project root, config, output sink for warnings, an opaque parser cache (so detectors can share a parsed AST per file).

### `Detector` trait (internal to each plugin)

```rust
pub trait Detector {
    fn id(&self) -> &'static str;            // e.g. "rust.file-size"
    fn run(&self, file: &ParsedFile) -> Vec<Finding>;
}
```

Each plugin owns a `Vec<Box<dyn Detector>>` and runs them in parallel via rayon over the file list. Detectors are pure: input `ParsedFile`, output `Vec<Finding>`. No I/O, no state. This makes them trivially unit-testable on string fixtures.

### `Finding`

```rust
pub struct Finding {
    pub detector: &'static str,        // "rust.file-size"
    pub severity: Severity,            // Low | Medium | High | Critical
    pub file: PathBuf,
    pub span: Option<Span>,            // line/col range, optional
    pub message: String,               // human-readable
    pub fix_hint: Option<String>,      // what an agent should do
    pub fingerprint: String,           // stable hash for dedup across runs
}
```

`fingerprint` is critical — it's how the engine recognizes "this is the same issue I saw last scan" so progress carries across runs. Computed as `blake3(detector_id || file_path || span.start.line || normalized_snippet)`. The choice of inputs is a deliberate tradeoff: too narrow and reformatting changes invalidate the fingerprint; too broad and we lose track of issues that genuinely moved. v0.1 uses the inputs above; we'll tune this once we have empirical data.

---

## Section 3 — Scoring

Goal: a single number 0-100. Higher is better. No gaming prevention in v0.1, just a clean baseline we can grow.

### Severity weights

| Severity | Weight |
|---|---|
| Low | 1 |
| Medium | 3 |
| High | 8 |
| Critical | 20 |

### Formula

```
penalty = sum(severity_weight(f) for f in findings)
size    = max(1, total_loc / 1000)         // findings normalized per kLoC
score   = 100 * exp(-penalty / (size * 25))
```

Why exponential decay: it makes each additional finding hurt less than the previous one (so a project with 200 issues isn't infinitely penalized vs one with 100), but a clean codebase with zero findings cleanly hits 100. The `25` is a tuning constant we'll calibrate empirically — for a "mediocre" 10kLoC codebase with maybe 30 findings, we want to land somewhere in the 60-80 range.

### Per-dimension breakdown (deferred but reserved)

The `Score` struct will carry per-detector subtotals, even in v0.1 where we only display the composite. This means we can add a `--detail` view later without changing the score data shape.

---

## Section 4 — State, queue, and "what to fix"

### Persistent state (`.dslp/state.json`)

```jsonc
{
  "version": 1,
  "last_scan": "2026-05-01T20:16:00Z",
  "score": 72.3,
  "findings": [
    {
      "fingerprint": "...",
      "detector": "rust.file-size",
      "severity": "medium",
      "file": "src/foo.rs",
      "first_seen": "2026-05-01T20:16:00Z",
      "last_seen": "2026-05-01T20:16:00Z",
      "resolved": false,
      "message": "...",
      "fix_hint": "..."
    }
  ]
}
```

A scan rebuilds the findings list, then merges by fingerprint with the previous state:
- New fingerprints → `first_seen = now`
- Existing fingerprints → update `last_seen`, preserve `first_seen`
- Fingerprints absent from current scan → mark `resolved = true`, keep in history

The `.dslp/` directory should be gitignored (we'll generate the entry).

### Queue ordering for `next`

`next` returns the highest-priority unresolved finding. Priority = severity weight, ties broken by `first_seen` ascending (older first — so we don't spin on the newest things forever). Output:

```
[medium] rust.file-size  src/foo.rs:1
  File has 540 lines (threshold: 400). Consider splitting along module boundaries.

  How to fix: identify a coherent sub-unit and extract it into a new module.

  When done: dslp resolve <fingerprint>     # not implemented in v0.1
```

`resolve` is reserved for a later slice; in v0.1 the user just edits and rescans.

---

## Section 5 — CLI surface

```
dslp scan [--path <dir>] [--lang <id>] [--config <path>]
    Scan a directory and write state to .dslp/state.json.
    Default --path is "." Default --lang auto-detects from file extensions present.

dslp next [--path <dir>]
    Print the highest-priority unresolved finding from the last scan.
    Exits 1 if no scan has been run yet.
```

That's it for v0.1. Reserved for later: `score`, `findings list`, `resolve`, `suppress`, `plan`, `backlog`, `update-skill`.

### Output format

Default is the human-readable format shown above. Reserved for later: `--json` for agent consumption.

### Configuration

`.dslp/config.toml` — optional. v0.1 keys:

```toml
[scan]
exclude = ["target/", "node_modules/"]   # in addition to gitignore

[detectors.file_size]
threshold = 400        # lines
severity_at = 600      # bumps to High above this

[detectors.complexity]
cyclomatic_threshold = 10
length_threshold = 50

[detectors.dead_pub]
ignore_paths = ["src/lib.rs"]   # entry points where unused-looking is fine
```

If the file is absent, defaults from `dslp-base/config.rs` apply.

---

## Section 6 — Testing strategy

- **Unit tests in each crate** for everything that can be tested in isolation. Detectors get string-fixture tests (parse a 10-line snippet, assert findings).
- **Integration tests in `dslp-cli/tests/`** that invoke the binary against `tests/fixtures/<repo>/` directories — small synthetic projects designed to trigger specific findings.
- **Dogfood test:** `dslp scan --path .` against this very repo, verifying it produces a sensible score and queue. Manual for v0.1, automated later.
- **No fidelity tests against desloppify** — we explicitly chose path B (our own detectors/scoring). Cross-implementation comparison is a non-goal.

### Coverage target

Not enforced in v0.1, but the bar is: every detector has at least one positive test (it fires when it should) and one negative test (it doesn't fire when it shouldn't).

---

## Decisions deferred

These are all explicit "we're not deciding now" calls. Each will need a follow-on spec.

- Anti-gaming (wontfix penalties, lenient vs strict score)
- Subjective dimensions (LLM review)
- Plan/triage workflow
- Cross-language monorepo support
- Scorecard badge / GitHub integration
- Update-skill mechanism (`dslp update-skill claude` in desloppify)
- Autofix layer

## Open risks

- **Tree-sitter Rust grammar quirks.** Some Rust syntax (e.g. macros, complex generics) parses as ERROR nodes. The `dead_pub` detector in particular may produce false positives if the grammar doesn't resolve all references. Mitigation: dogfood early, lower severity for `dead_pub` to `Low` if precision is poor in v0.1.
- **Scoring constant calibration.** The `25` in the score formula is a guess. We'll need to run on a handful of real codebases (this one, ripgrep, a known-bad codebase) to validate it doesn't always return 99 or always return 12.
- **Fingerprint stability.** If files get reformatted, line numbers shift and findings appear to "move." For v0.1 this is acceptable noise; a later slice can switch to AST-anchored fingerprints.
