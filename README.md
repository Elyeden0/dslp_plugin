# noAIslop (`dslp`)

A Rust port of the analyse → score → "what to fix" core of [desloppify](https://github.com/peteromallet/desloppify). Scans your codebase, scores it 0-100, and tells your AI agent (or you) what to fix next.

This is **v0.12 in progress**: Rust + TypeScript/JavaScript + Go + Python + C + C++ language support plus React, Next.js, and Nuxt framework-aware detectors that auto-activate on the right projects, composite score 0-100, persistent state, a resolve/suppress workflow loop, and per-language monorepo scoping. See [docs/superpowers/specs/2026-05-01-rust-port-design.md](docs/superpowers/specs/2026-05-01-rust-port-design.md) and [docs/superpowers/specs/2026-05-29-workflow-loop-design.md](docs/superpowers/specs/2026-05-29-workflow-loop-design.md) for the full design and what's deliberately deferred.

## Install

```bash
git clone <this-repo> noAIslop
cd noAIslop
cargo install --path dslp-cli
```

After install, the binary is `dslp`.

## Usage

```bash
# Once per project: starter .dslp/config.toml + ignore .dslp/ in .gitignore
dslp init --path /path/to/project

# Scan a project (writes state to .dslp/state.json)
dslp scan --path /path/to/project

# Show the highest-priority open issue
dslp next --path /path/to/project

# Full detail on one issue, whatever its status (shows the wontfix reason)
dslp show <fingerprint> --path /path/to/project

# Print the score: lenient / strict / gap (no rescan)
dslp score --path /path/to/project

# List all open issues in priority order
dslp findings --path /path/to/project
dslp findings --limit 10 --path /path/to/project
dslp findings --sarif --path /path/to/project > dslp.sarif   # GitHub code scanning etc.

# Batch triage: open issues grouped by file, ranked by total severity penalty
dslp plan --path /path/to/project
dslp plan --limit 5 --path /path/to/project

# The wontfix backlog: standing decisions to not fix something
dslp backlog --path /path/to/project

# Emit a shields.io endpoint-badge payload (strict score by default)
dslp badge --path /path/to/project > badge.json
# ![dslp](https://img.shields.io/endpoint?url=<raw-url-of-badge.json>)

# Work the queue: mark an issue fixed, or dismiss it
dslp resolve <fingerprint> --path /path/to/project
dslp suppress <fingerprint> --reason "intentional here" --path /path/to/project
# Undo either: put an issue back in the queue
dslp reopen <fingerprint> --path /path/to/project

# CI gating: exit 1 if strict < 80, or if this scan found new/regressed issues
dslp scan --fail-under 80 --fail-on-new --path .

# Monorepo: score one language independently of the others under the same --path
dslp scan --lang go --path /path/to/monorepo
dslp findings --lang go --path /path/to/monorepo
```

Dismiss an issue in the code itself with a comment on its first line, or the line above (any comment syntax):

```rust
// dslp-ignore rust.complexity: generated dispatch table
fn dispatch(op: Op) -> u32 { ... }
```

Several ids may be listed (`dslp-ignore rust.params, rust.length`) and a family can end in `.*`. Like `suppress`, an inline ignore becomes a **wontfix** — it shows in `dslp backlog` and still counts toward the strict score. Deleting the comment later does not reopen the issue; use `dslp reopen`.

Fingerprints accept a unique prefix (the 8-char id shown in `findings`/`next`).

Every command supports `--json` for machine-readable output suitable for piping into an agent (`badge` has no `--json` flag — its only output is the badge payload itself):

```bash
dslp scan --json --path .
dslp next --json --path .   # full Issue payload, with fingerprint
dslp score --json --path .  # composite + per-detector breakdown
dslp findings --json --path .   # array of all unresolved Issues
dslp plan --json --path .   # array of { file, penalty, issue_count, highest_severity, oldest }
dslp backlog --json --path .   # array of wontfix Issues
```

`--fail-on-new` compares against the previous scan's state, so it only has something to compare to where `.dslp/` persists between runs (e.g. a cached CI directory); the first scan never fails it. Add `.dslp/` to your `.gitignore`. State persists across runs so progress carries over.

## What it detects

### Rust (`.rs`)

| Detector | What it flags |
|---|---|
| `rust.file-size` | Files over 400 lines (configurable). Severity bumps to High at 600. |
| `rust.complexity` | Functions with cyclomatic complexity ≥ 10 (counts `if`, `match` arms, loops, `?`, `&&`, `\|\|`). |
| `rust.length` | Functions ≥ 50 lines. Low severity. |
| `rust.params` | Functions with ≥ 6 parameters (the `self` receiver is free). Low severity. |
| `rust.nesting` | Functions nesting `if` / `match` / loops 5+ levels deep (`else if` / `elif` chains do not add depth). Low severity. |
| `rust.leftover-debug` | `dbg!` left in code. Low severity. |
| `rust.unfinished` | `todo!()` / `unimplemented!()`, which panic at runtime. Medium severity. |
| `rust.dead-pub-fn` / `-struct` / `-enum` / `-trait` | Public items whose name isn't referenced anywhere in the corpus. Low severity (heuristic — false positives possible on macro-generated or trait-dispatched code). |

### TypeScript / JavaScript (`.ts`, `.tsx`, `.js`, `.jsx`, `.cjs`, `.mjs`)

| Detector | What it flags |
|---|---|
| `ts.file-size` | Files over the threshold (same configurable thresholds as Rust). |
| `ts.complexity` | Functions / arrow functions / methods with cyclomatic complexity ≥ 10. Counts `if`, `for`, `while`, `do`, `switch case`, `?:`, `catch`, `&&`, `\|\|`, `??`. |
| `ts.length` | Functions ≥ 50 lines. Low severity. |
| `ts.params` | Functions with ≥ 6 parameters. Low severity. |
| `ts.nesting` | Functions nesting `if` / `switch` / `try` / loops 5+ levels deep (`else if` / `elif` chains do not add depth). Low severity. |
| `ts.empty-catch` | A `catch` block with no statements at all (a comment inside counts as a deliberate ignore). Medium severity. |
| `ts.leftover-debug` | `console.log` / `console.debug` / `console.trace` calls and `debugger` statements. Low severity. |

A "dead-export" detector for TS/JS is intentionally not in this slice — the TS module system (default exports, re-exports, dynamic imports, JSX references) needs its own design pass.

### Go (`.go`)

| Detector | What it flags |
|---|---|
| `go.file-size` | Files over the threshold. |
| `go.complexity` | Functions and methods with cyclomatic complexity ≥ 10. Counts `if`, `for`, switch `expression_case` / `default_case`, `select` `communication_case`, type-switch `type_case`, plus `&&` / `\|\|`. |
| `go.length` | Functions ≥ 50 lines. Low severity. |
| `go.params` | Functions with ≥ 6 parameters (`a, b int` counts as two). Low severity. |
| `go.nesting` | Functions nesting `if` / `switch` / `select` / loops 5+ levels deep (`else if` / `elif` chains do not add depth). Low severity. |

### Python (`.py`, `.pyi`)

| Detector | What it flags |
|---|---|
| `python.file-size` | Files over the threshold. |
| `python.complexity` | Functions / methods (including nested defs) with cyclomatic complexity ≥ 10. Counts `if`, `elif`, `for`, `while`, `except`, `match` `case`, conditional expressions (`x if c else y`), plus `and` / `or`. |
| `python.length` | Functions ≥ 50 lines. Low severity. |
| `python.params` | Functions with ≥ 6 parameters (`self`/`cls` are free). Low severity. |
| `python.nesting` | Functions nesting `if` / `try` / `with` / `match` / loops 5+ levels deep (`else if` / `elif` chains do not add depth). Low severity. |
| `python.silent-except` | An `except` block containing only `pass` / `...`. Medium severity. |
| `python.leftover-debug` | `breakpoint()`, `pdb.set_trace()`, `ipdb.set_trace()`. Low severity. |

### C (`.c`, `.h`)

| Detector | What it flags |
|---|---|
| `c.file-size` | Files over the threshold. |
| `c.complexity` | Functions with cyclomatic complexity ≥ 10. Counts `if`, `for`, `while`, `do`, `case`, ternary `?:`, `&&`, `\|\|`. Preprocessor `#ifdef`/`#if` are not counted (conditional compilation is a different axis). |
| `c.length` | Functions ≥ 50 lines. Low severity. |

### C++ (`.cpp`, `.cc`, `.cxx`, `.c++`, `.hpp`, `.hh`, `.hxx`, `.h++`)

| Detector | What it flags |
|---|---|
| `cpp.file-size` | Files over the threshold. |
| `cpp.complexity` | Functions / methods with cyclomatic complexity ≥ 10. Same as C plus `for_range_loop` and `catch_clause`. Handles qualified names (`Foo::bar`), operator overloads, and destructors. |
| `cpp.length` | Functions ≥ 50 lines. Low severity. |

`.h` files belong to the C plugin (the dominant convention for C headers). C++ headers should use `.hpp` to be picked up by the C++ plugin.

### React (auto-activated on React projects)

Activates when a project's `package.json` includes any of `react` / `react-dom` / `next` / `gatsby` / `remix` (or any of those in dev/peer deps), or when the file set contains at least one `.tsx` file.

| Detector | What it flags |
|---|---|
| `react.hook-return-bloat` | Custom hooks (`useXxx`) whose final `return` is an object literal with > 12 fields. Threshold matches desloppify's reference detector. The signal: a hook bundling too many concerns is a refactor candidate. |

### Next.js (auto-activated when `next` is in `package.json`)

| Detector | What it flags |
|---|---|
| `nextjs.use-client-not-first` | A `'use client'` directive that isn't the first meaningful line of the module. Next.js silently treats such files as server components — a hard-to-spot bug. High severity. Tolerates leading line/block comments and a shebang. |

### Nuxt (auto-activated when `nuxt` is in `package.json`) — net-new

This is **net-new design** — desloppify ships no Nuxt support, so we couldn't crib from it.

| Detector | What it flags |
|---|---|
| `nuxt.composable-naming` | A TS/JS file inside a `composables/` directory whose every exported function violates Nuxt's `use[A-Z]` naming convention. Auto-import won't pick those exports up under the expected name. The file passes if at least one of its exported functions is well-named. |

### Duplicates (all languages)

| Detector | What it flags |
|---|---|
| `dup.block` | A run of ≥ 8 identical significant lines (blank lines, comments, imports and lone braces are ignored) that also appears elsewhere. Of N copies, N−1 are reported, each pointing at the first. Test files (`tests/`, `*_test.*`, `*.spec.*`, `test_*`) are skipped. Low severity, Medium at 20+ lines. Tune with `[detectors.duplicates] min_lines`. |

## Configuration

Drop a `.dslp/config.toml` in your project root:

```toml
[scan]
exclude = ["target/", "node_modules/", "vendor/"]

[detectors]
disable = ["rust.dead-pub-fn", "ts.*"]   # exact ids, or a family with a trailing .*

[detectors.severity]
"rust.params" = "medium"                 # low | medium | high | critical

[detectors.languages.python]             # per-language overrides, keyed by plugin id
cyclomatic_threshold = 15                # also: length_threshold, param_threshold,
file_size_threshold = 600                #       nesting_threshold, file_size_severity_at

[detectors.duplicates]
min_lines = 8

[detectors.file_size]
threshold = 400
severity_at = 600

[detectors.complexity]
cyclomatic_threshold = 10
length_threshold = 50
param_threshold = 6
nesting_threshold = 5

[detectors.dead_pub]
ignore_paths = ["src/lib.rs"]
```

Every key is optional; missing values fall back to defaults. Unknown keys are an error, so a typo like `treshold` is reported rather than silently ignored. `exclude` entries are path prefixes matched per path component (`"vendor"` and `"vendor/"` exclude `vendor/x.rs` but not `vendor2/x.rs`).

## Scoring

Two scores, both 0 (worst) to 100 (perfect), from the same formula:

```
penalty = sum of severity weights (Low=1, Medium=3, High=8, Critical=20)
size    = max(1, total_loc / 1000)
score   = 100 * exp(-penalty / (size * 25))
```

- **lenient** counts only Open issues — suppressing (wontfix) an issue raises it.
- **strict** counts Open **and** Wontfix issues — dismissing an issue does not improve it.
- **gap** = lenient − strict. A clean codebase has both at 100, gap 0. A codebase where you've dismissed real issues has an inflated lenient and an exposing gap. This is the anti-gaming mechanic: you can't reach a real 100 by suppressing everything, because strict stays low.

Larger codebases tolerate more penalty before the score drops. Subjective LLM dimensions are still deferred to a later slice.

## Issue lifecycle

Each issue has a status:

- **Open** — actionable; counts against the score and appears in `next`/`findings`.
- **Resolved** — `dslp resolve` marks it optimistically fixed. It's removed from the queue, but a later `scan` that re-detects the fingerprint **reopens it as a regression** — `resolve` is a claim the next scan verifies.
- **Wontfix** — `dslp suppress --reason` dismisses it as a standing decision. It survives rescans, stays out of the queue, but still counts toward `strict`. `dslp backlog` is the dedicated view onto these — `findings`/`plan` show Open issues only.

State persists in `.dslp/state.json` (schema v3). State from older versions is discarded on load — the v3 fingerprint change means old issue ids would never match again, so upgrading starts a fresh history (re-apply any `suppress` afterwards). Issue fingerprints are built from the detector, the project-relative file path, and the flagged item's name — not the line number or measured value — so an issue keeps its identity (and a `suppress` keeps applying) when you edit code above it. Resolved issues that stay gone for 90 days are pruned from state. `dslp reopen` reverses `resolve` and `suppress`.

`dslp next`/`findings` rank individual issues; `dslp plan` ranks *files* — open issues grouped by file, sorted by the sum of their severity weights, so an agent can fix a whole file's worth of issues in one pass instead of working the flat queue one at a time.

## Monorepo support (`--lang`)

Every command accepts `--lang <id>` (`rust`, `typescript`, `go`, `python`, `c`, `cpp`, `react`, `nextjs`, `nuxt`, `duplicates`) to scope it to one plugin. A scoped `scan` only runs that plugin and persists to its own `.dslp/state-<lang>.json`, completely independent of the shared `.dslp/state.json` — so a monorepo with e.g. a Go backend and a TypeScript frontend under the same `--path` can track and score each language separately (`dslp scan --lang go`, `dslp scan --lang typescript`) without one language's fixes or regressions touching the other's history. Omit `--lang` to scan/score everything under `--path` together, as before. `--lang` with an unrecognized id fails fast, listing the valid ids.

## Architecture

Cargo workspace:

- `dslp-core` — domain types (`Issue`, `Finding`, `Score`, `LanguagePlugin` trait). No I/O, no parsers.
- `dslp-base` — file discovery (gitignore-aware), config loading, tree-sitter glue.
- `dslp-engine` — scan/score/queue/plan/backlog/badge/state pipeline.
- `dslp-lang-common` — shared detector primitives (`count_lines`, `cyclomatic`, `span_of`, `node_length_lines`, `finding_at_node`, `file_size`) used by every language plugin.
- `dslp-lang-rust` — Rust language plugin.
- `dslp-lang-ts` — TypeScript / JavaScript language plugin.
- `dslp-lang-go` — Go language plugin.
- `dslp-lang-python` — Python language plugin.
- `dslp-lang-c` — C language plugin.
- `dslp-lang-cpp` — C++ language plugin.
- `dslp-fw-react` — React framework plugin (auto-detects react/next/gatsby/remix in `package.json` or any `.tsx` file).
- `dslp-fw-nextjs` — Next.js framework plugin (auto-detects `next` in `package.json`).
- `dslp-fw-nuxt` — Nuxt framework plugin (net-new — auto-detects `nuxt` in `package.json`).
- `dslp-dup` — cross-language duplicate-block detector.
- `dslp-cli` — the `dslp` binary.

## Roadmap

In order:

1. ✅ v0.1 — Rust language plugin
2. ✅ v0.2 — TypeScript / JavaScript language plugin (file-size + complexity + length; dead-export deferred)
3. ✅ v0.3 — React framework plugin (`hook-return-bloat`) with self-gating detection
4. ✅ v0.4 — Next.js framework plugin (`use-client-not-first`)
5. ✅ v0.5 — Nuxt framework plugin (`composable-naming`) — net-new, no desloppify reference
6. ✅ v0.6 — Go language plugin (`file-size`, `complexity`, `length`); split CLI integration tests one-per-plugin
7. ✅ v0.7 — Python language plugin (`file-size`, `complexity`, `length`) — handles `match` statements and nested defs
8. ✅ v0.8 — C and C++ language plugins (`file-size`, `complexity`, `length`); the original 7-language order is now complete
9. ✅ v0.9 — Workflow loop: `resolve` / `suppress` commands, Open/Resolved/Wontfix status model, lenient-vs-strict anti-gaming score
10. ✅ `dslp-lang-common` extraction — shared `count_lines`/`cyclomatic`/`span_of`/`node_length_lines`/`finding_at_node`/`file_size` primitives, all 6 language plugins wired to use them
11. ✅ v0.10 — Triage workflow: `plan` (open issues grouped by file, ranked by total severity penalty) and `backlog` (the wontfix list, previously invisible outside raw state)
12. ✅ v0.11 — Scorecard badge: `dslp badge` emits a shields.io endpoint-badge payload, strict score by default
13. ✅ v0.12 — `--lang` monorepo support: every command can scope to one plugin, tracked in its own `.dslp/state-<lang>.json`
14. Later: LLM-backed subjective review; autofix.

## Performance

A scan walks the tree once, reads each file once, and runs the plugins in parallel; parsing is tree-sitter with no ASTs round-tripped to disk. Measure it yourself:

```bash
cargo build --release --bin dslp
time ./target/release/dslp scan --path .
```

## Testing

```bash
cargo test --workspace
```

203 tests as of v0.12. v0.8 added C/C++ parser glue and the `c.*`/`cpp.*` detectors with shared declarator-identifier extraction. v0.9 adds the workflow loop: the `Status` enum and its serde, the six-row merge truth table (Open/Resolved/Wontfix × re-detected/not), lenient-vs-strict scoring properties (suppressing widens the gap, resolving improves both, lenient ≥ strict always), fingerprint-prefix matching, `resolve`/`suppress` ops, v1-state-discard-on-load, and end-to-end CLI tests for the full scan → suppress → rescan-preserves-wontfix flow. v0.10 adds `plan` (grouping-by-file, penalty ranking, tie-breaking by oldest `first_seen`, Wontfix/Resolved exclusion) and `backlog` (Wontfix-only filtering, most-recently-suppressed-first ordering) with unit and CLI end-to-end coverage. v0.11 adds `badge`: color-threshold table, strict-vs-lenient score selection, and a no-scan placeholder payload. v0.12 adds `--lang`: per-language state file scoping, an unknown-lang error path, and end-to-end coverage for scoped scan/findings/resolve independence from the unscoped state.

## License

Same dual MIT-or-Apache-2 as is conventional for Rust workspaces. See `LICENSE` (TODO: drop one in).
