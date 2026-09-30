# noAIslop (`dslp`)

A Rust port of the analyse → score → "what to fix" core of [desloppify](https://github.com/peteromallet/desloppify). Scans your codebase, scores it 0-100, and tells your AI agent (or you) what to fix next.

**v0.13, in progress.** `dslp` analyses a project with tree-sitter, turns what it finds into tracked *issues*, and keeps a 0–100 score you can gate CI on.

- **Languages:** Rust, TypeScript/JavaScript, Go, Python, C, C++, plus React, Next.js and Nuxt detectors that switch on automatically.
- **Slop-focused checks:** duplicated code, commented-out code, swallowed errors, leftover debugging, long parameter lists, deep nesting, and more (see [What it detects](#what-it-detects)).
- **Workflow:** issues persist between scans; you resolve, suppress (or `dslp-ignore` in code) and reopen them, and a *lenient vs strict* score keeps dismissals honest.
- **CI:** `--fail-under`, `--fail-on-new`, `--since <rev>`, SARIF output, a shields.io badge, and per-language scoping for monorepos.

## Install

Requires a Rust toolchain (`rust-version = 1.78` in `Cargo.toml`).

```bash
git clone https://github.com/Elyeden0/rust_noAIslop.git noAIslop
cd noAIslop
cargo install --path dslp-cli
```

After install, the binary is `dslp`.

## Quick start

```bash
dslp init                      # once: starter .dslp/config.toml, and ignores state files in .gitignore
dslp scan                      # analyse; prints the score and what changed since last time
dslp next                      # the single most important open issue, with how to fix it
# ...fix it, then:
dslp resolve <fingerprint>     # claim it's fixed (the next scan verifies the claim)
dslp scan                      # repeat
```

Can't or won't fix one? `dslp suppress <fingerprint> --reason "..."` (or a `dslp-ignore` comment in the code) dismisses it as a *wontfix*. That still counts against the **strict** score, so it can't be used to game the number.

## Usage

Every command takes the project directory either as a positional argument (`dslp scan ./project`) or as `--path ./project` (default: the current directory). Giving both is an error. The examples below use `--path`.

```bash
# Once per project: starter .dslp/config.toml + ignore the state files (.dslp/state*) in .gitignore
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

# PR-style scan: report only on files changed since a git revision
dslp scan --since origin/main --fail-on-new --path .

# Monorepo: score one language independently of the others under the same --path
dslp scan --lang go --path /path/to/monorepo
dslp findings --lang go --path /path/to/monorepo
```

Dismiss an issue in the code itself with a `dslp-ignore` comment on the line where the issue starts, or on the line directly above it (any comment syntax):

```rust
// dslp-ignore rust.complexity: generated dispatch table
fn dispatch(op: Op) -> u32 { ... }
```

Several ids may be listed (`dslp-ignore rust.params, rust.length`) and a family can end in `.*`. Like `suppress`, an inline ignore becomes a **wontfix** — it shows in `dslp backlog` and still counts toward the strict score. Deleting the comment later does not reopen the issue; use `dslp reopen`.

Fingerprints accept a unique prefix (the 8-char id shown in `findings`/`next`).

Every command except `init` and `badge` supports `--json` for machine-readable output suitable for piping into an agent (`badge` already prints JSON — the badge payload; `findings` can also print `--sarif`):

```bash
dslp scan --json --path .
dslp next --json --path .   # full Issue payload, with fingerprint
dslp score --json --path .  # composite + per-detector breakdown
dslp findings --json --path .   # array of all unresolved Issues
dslp plan --json --path .   # array of { file, penalty, issue_count, highest_severity, oldest }
dslp backlog --json --path .   # array of wontfix Issues
```

`--fail-on-new` compares against the previous scan's state, so it only has something to compare to where `.dslp/` persists between runs (e.g. a cached CI directory); the first scan never fails it. `--since <rev>` still analyses the whole project (cross-file detectors like `rust.dead-pub-*`, `ts.dead-export` and `dup.block` need the full corpus) but only *reports* on files that differ from `<rev>` in the working tree, plus untracked files; issues in every other file are carried over exactly as they were. Findings in unchanged files that a change elsewhere caused are not seen until the next full scan.

State persists in `.dslp/` across runs so progress carries over. Commit `.dslp/config.toml` to share settings with your team, but keep the state files (`.dslp/state*.json`) out of git — `dslp init` sets that up.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. |
| `1` | A check said "no": a `scan` gate failed (`--fail-under` / `--fail-on-new`), there is no scan yet, or `next` found no open issues. |
| `2` | Error or bad usage (unknown `--lang`, unreadable config, bad `--since` revision, ...). |

## What it detects

Every detector has an id like `rust.complexity` and a severity (low, medium, high, critical) that feeds the [score](#scoring). Turn any of them off or change its severity in the [config](#configuration).

### At a glance

| Check | Rust | TS/JS | Go | Python | C | C++ |
|---|---|---|---|---|---|---|
| File size | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Cyclomatic complexity | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Function length | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Too many parameters | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Deep nesting | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Commented-out code, comments that restate the code | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| Swallowed errors | | `ts.empty-catch` | `go.ignored-error` | `python.silent-except` | | `cpp.empty-catch` |
| Leftover debugging | `rust.leftover-debug` | `ts.leftover-debug` | | `python.leftover-debug` | | |
| Unfinished code | `rust.unfinished` | | | | | |
| Habit density (per file) | `rust.unwrap-density` | `ts.any-density` | | | | |
| Unused public items | `rust.dead-pub-*` | `ts.dead-export` | | | | |

Across all languages, `dup.block` finds duplicated code. Framework detectors switch on automatically: React (`react.hook-return-bloat`), Next.js (`nextjs.use-client-not-first`), Nuxt (`nuxt.composable-naming`).

Details, thresholds and exact rules are below; click a language to expand.

<details>
<summary><b>Rust (.rs)</b></summary>

| Detector | What it flags |
|---|---|
| `rust.file-size` | Files over 400 lines (configurable). Severity bumps to High at 600. |
| `rust.complexity` | Functions with cyclomatic complexity ≥ 10. Counts `if`, loops, `?`, `&&`, `\|\|` and `match` arms (a `match` with n arms adds n − 1). Nested named `fn`s are scored on their own, not added to the parent. |
| `rust.length` | Functions ≥ 50 lines. Low severity. |
| `rust.params` | Functions with ≥ 6 parameters (the `self` receiver is free). Low severity. |
| `rust.nesting` | Functions nesting `if` / `match` / loops 5+ levels deep (`else if` chains do not add depth). Low severity. |
| `rust.leftover-debug` | `dbg!` left in code. Low severity. |
| `rust.unfinished` | `todo!()` / `unimplemented!()`, which panic at runtime. Medium severity. |
| `rust.unwrap-density` | A file with ≥ 10 `.unwrap()` / `.expect()` calls outside `#[test]` / `mod tests` / `tests/`. One finding per file. Low severity. |
| `rust.dead-pub-fn` / `-struct` / `-enum` / `-trait` | Public items whose name isn't referenced anywhere in the corpus. Low severity (heuristic — false positives possible on macro-generated or trait-dispatched code). |

</details>

<details>
<summary><b>TypeScript / JavaScript (.ts, .tsx, .js, .jsx, .cjs, .mjs)</b></summary>

| Detector | What it flags |
|---|---|
| `ts.file-size` | Files over the threshold (same configurable thresholds as Rust). |
| `ts.complexity` | Functions / arrow functions / methods with cyclomatic complexity ≥ 10. Counts `if`, `for`, `while`, `do`, `switch case`, `?:`, `catch`, `&&`, `\|\|`, `??`. |
| `ts.length` | Functions ≥ 50 lines. Low severity. |
| `ts.params` | Functions with ≥ 6 parameters. Low severity. |
| `ts.nesting` | Functions nesting `if` / `switch` / `try` / loops 5+ levels deep (`else if` chains do not add depth). Low severity. |
| `ts.empty-catch` | A `catch` block with no statements at all (a comment inside counts as a deliberate ignore). Medium severity. |
| `ts.leftover-debug` | `console.log` / `console.debug` / `console.trace` calls and `debugger` statements. Low severity. |
| `ts.any-density` | A file with ≥ 5 explicit `any` types. One finding per file. Low severity. |
| `ts.dead-export` | `export function/const/class/interface/type/enum` whose name isn't mentioned anywhere else in the project (see notes below). Low severity. |

**`ts.dead-export` notes.** It matches names only and counts any mention (import, re-export, local use, JSX, type) as a reference, so it errs towards silence. It is skipped for library packages (`package.json` with `main` / `module` / `exports` / `types`), for `export default` and `export { .. }` clauses, and for files a framework loads by convention (`index`, `page`, `layout`, `route`, `pages/`, `app/`, `components/`, `composables/`, `server/`, `utils/`, `*.config.*`, `*.d.ts`, tests). `detectors.dead_pub.ignore_paths` applies.

</details>

<details>
<summary><b>Go (.go)</b></summary>

| Detector | What it flags |
|---|---|
| `go.file-size` | Files over the threshold. |
| `go.complexity` | Functions and methods with cyclomatic complexity ≥ 10. Counts `if`, `for`, switch `expression_case` / `default_case`, `select` `communication_case`, type-switch `type_case`, plus `&&` / `\|\|`. |
| `go.length` | Functions ≥ 50 lines. Low severity. |
| `go.params` | Functions with ≥ 6 parameters (`a, b int` counts as two). Low severity. |
| `go.nesting` | Functions nesting `if` / `switch` / `select` / loops 5+ levels deep (`else if` chains do not add depth). Low severity. |
| `go.ignored-error` | `_ = f()` / `_, _ = f()`: a call whose result is assigned only to the blank identifier, discarding a possible error. Medium severity. |

</details>

<details>
<summary><b>Python (.py, .pyi)</b></summary>

| Detector | What it flags |
|---|---|
| `python.file-size` | Files over the threshold. |
| `python.complexity` | Functions / methods (nested defs are scored on their own) with cyclomatic complexity ≥ 10. Counts `if`, `elif`, `for`, `while`, `except`, `match` `case` (a `match` with n cases adds n − 1), conditional expressions (`x if c else y`), plus `and` / `or`. |
| `python.length` | Functions ≥ 50 lines. Low severity. |
| `python.params` | Functions with ≥ 6 parameters (`self`/`cls` are free). Low severity. |
| `python.nesting` | Functions nesting `if` / `try` / `with` / `match` / loops 5+ levels deep (`elif` chains do not add depth). Low severity. |
| `python.silent-except` | An `except` block containing only `pass` / `...`. Medium severity. |
| `python.leftover-debug` | `breakpoint()`, `pdb.set_trace()`, `ipdb.set_trace()`. Low severity. |

</details>

<details>
<summary><b>C (.c, .h)</b></summary>

| Detector | What it flags |
|---|---|
| `c.file-size` | Files over the threshold. |
| `c.complexity` | Functions with cyclomatic complexity ≥ 10. Counts `if`, `for`, `while`, `do`, `case`, ternary `?:`, `&&`, `\|\|`. Preprocessor `#ifdef`/`#if` are not counted (conditional compilation is a different axis). |
| `c.length` | Functions ≥ 50 lines. Low severity. |
| `c.params` | Functions with ≥ 6 parameters. Low severity. |
| `c.nesting` | Functions nesting `if` / `switch` / loops 5+ levels deep (`else if` chains do not add depth). Low severity. |

</details>

<details>
<summary><b>C++ (.cpp, .cc, .cxx, .c++, .hpp, .hh, .hxx, .h++)</b></summary>

| Detector | What it flags |
|---|---|
| `cpp.file-size` | Files over the threshold. |
| `cpp.complexity` | Functions / methods with cyclomatic complexity ≥ 10. Same as C plus `for_range_loop` and `catch_clause`. Handles qualified names (`Foo::bar`), operator overloads, and destructors. |
| `cpp.length` | Functions ≥ 50 lines. Low severity. |
| `cpp.params` | Functions and methods with ≥ 6 parameters. Low severity. |
| `cpp.nesting` | Same as `c.nesting`, plus range-`for` and `try`. Low severity. |
| `cpp.empty-catch` | A `catch` block with no statements (a comment inside counts as deliberate). Medium severity. |

`.h` files belong to the C plugin (the dominant convention for C headers). C++ headers should use `.hpp` to be picked up by the C++ plugin.

</details>

<details>
<summary><b>React (auto-activated on React projects)</b></summary>

Activates when a project's `package.json` includes any of `react` / `react-dom` / `next` / `gatsby` / `remix` (or any of those in dev/peer deps), or when the file set contains at least one `.tsx` file.

| Detector | What it flags |
|---|---|
| `react.hook-return-bloat` | Custom hooks (`useXxx`) whose final `return` is an object literal with > 12 fields. Threshold matches desloppify's reference detector. The signal: a hook bundling too many concerns is a refactor candidate. |

</details>

<details>
<summary><b>Next.js (auto-activated when next is in package.json)</b></summary>

| Detector | What it flags |
|---|---|
| `nextjs.use-client-not-first` | A `'use client'` directive that isn't the first meaningful line of the module. Next.js silently treats such files as server components — a hard-to-spot bug. High severity. Tolerates leading line/block comments and a shebang. |

</details>

<details>
<summary><b>Nuxt (auto-activated when nuxt is in package.json) — net-new</b></summary>

This is **net-new design** — desloppify ships no Nuxt support, so we couldn't crib from it.

| Detector | What it flags |
|---|---|
| `nuxt.composable-naming` | A TS/JS file inside a `composables/` directory whose every exported function violates Nuxt's `use[A-Z]` naming convention. Auto-import won't pick those exports up under the expected name. The file passes if at least one of its exported functions is well-named. |

</details>

<details>
<summary><b>Comment slop (Rust, TS/JS, Go, Python, C, C++)</b></summary>

| Detector | What it flags |
|---|---|
| `<lang>.commented-code` | 3+ consecutive `//` (or `#`) comment lines that read as code. Doc comments, shebangs and tool directives (`noqa`, `type:`, `eslint`, ...) are ignored. Low severity. |
| `<lang>.obvious-comment` | A 2–5 word comment whose words all appear in the identifiers of the line right below it (`// return the result` above `return result;`). Deliberately conservative — it misses paraphrases rather than flag explanations. Low severity. |

</details>

<details>
<summary><b>Duplicates (all languages)</b></summary>

| Detector | What it flags |
|---|---|
| `dup.block` | A run of ≥ 8 identical significant lines (blank lines, comments, imports and lone braces are ignored) that also appears elsewhere. Of N copies, N−1 are reported, each pointing at the first. Test files (`tests/`, `*_test.*`, `*.spec.*`, `test_*`) are skipped. Low severity, Medium at 20+ lines. Tune with `[detectors.duplicates] min_lines`. |

</details>

## Configuration

Create it with `dslp init` (it writes every default), or add a `.dslp/config.toml` in your project root by hand. The example below shows every option; values here are illustrative, not all defaults (e.g. the `disable` and `ignore_paths` lists are empty by default):

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

[detectors.slop]
unwrap_threshold = 10
any_threshold = 5

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

Every key is optional; missing values fall back to defaults. Unknown keys are an error, so a typo like `treshold` is reported rather than silently ignored. `dead_pub.ignore_paths` entries are substring matches on the file path. `exclude` entries are path prefixes matched per path component (`"vendor"` and `"vendor/"` exclude `vendor/x.rs` but not `vendor2/x.rs`).

## Scoring

Two scores, both 0 (worst) to 100 (perfect), from the same formula:

```
penalty = sum of severity weights (Low=1, Medium=3, High=8, Critical=20)
size    = max(1, total_loc / 1000)      # integer division: thousands of lines, at least 1
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

Every command accepts `--lang <id>` (`rust`, `typescript`, `go`, `python`, `c`, `cpp`, `react`, `nextjs`, `nuxt`, `duplicates`) to scope it to one plugin. (The duplicate detector is the plugin `duplicates`, but its findings are ids `dup.*`.)

A scoped `scan` runs only that plugin and keeps its own state in `.dslp/state-<lang>.json`, independent of the shared `.dslp/state.json`. A monorepo with, say, a Go backend and a TypeScript frontend can therefore track and score each separately (`dslp scan --lang go`, `dslp scan --lang typescript`) without one language's fixes or regressions touching the other's history.

Omit `--lang` to scan and score everything under `--path` together. An unrecognized id fails fast and lists the valid ones.

## Architecture

Cargo workspace:

- `dslp-core` — domain types (`Issue`, `Finding`, `Score`, `LanguagePlugin` trait). No I/O, no parsers.
- `dslp-base` — file discovery (gitignore-aware), config loading, tree-sitter glue.
- `dslp-engine` — scan pipeline (single file walk, parallel plugins), state merge, scoring, queue/plan/backlog/badge, scan deltas, config overrides, inline `dslp-ignore`, SARIF.
- `dslp-lang-common` — shared detector primitives used by every language plugin: line/length/complexity helpers, the file-size detector, parameter-count and nesting-depth helpers, C-family declarator names, and the comment-slop detectors.
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

## Status and roadmap

All six languages, the three framework plugins, the workflow loop (resolve / suppress / reopen / inline ignores), CI gating and the slop detectors are implemented. What's next, roughly in order (closures still count toward their parent function's complexity, on purpose):

1. **Faster partial scans** — `--since` only limits what is *reported*; the whole project is still analysed so cross-file detectors stay correct. Real speed-ups need per-detector "needs the whole corpus" metadata.
2. **Smarter duplicate detection** — `dup.block` compares normalised lines, so clones with renamed variables slip through; a token-based pass would catch them.
3. **More slop coverage** — leftover-debug and density checks for Go, C and C++ (printing is legitimate there, so it needs a policy).
4. **LLM-backed subjective review and autofix** — deliberately deferred.

<details>
<summary>Version history</summary>

1. ✅ v0.1 — Rust language plugin
2. ✅ v0.2 — TypeScript / JavaScript language plugin (file-size + complexity + length; unused exports came later, in v0.13)
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
14. ✅ v0.13 — CI and slop: scan change summary, `--fail-under` / `--fail-on-new` / `--since`, SARIF, `show` / `reopen` / `init`, `dslp-ignore` comments, richer config, and a batch of new detectors (see [What it detects](#what-it-detects))

</details>

## Performance

A scan walks the tree once, reads each file once, and runs the plugins in parallel; parsing is tree-sitter with no ASTs round-tripped to disk. As a reference point, this workspace (~11k lines) scans in about 0.4 s in a release build on the author's machine. Measure your own:

```bash
cargo build --release --bin dslp
time ./target/release/dslp scan --path .
```

## Testing

```bash
cargo test --workspace
```

274 tests as of v0.13: unit tests next to the code they cover (detectors, engine merge/scoring, config) and 51 end-to-end tests in `dslp-cli/tests/` that run the real binary against temporary projects.

## License

Released under the [MIT License](LICENSE).
