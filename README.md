# noAIslop (`dslp`)

A Rust port of the analyse → score → "what to fix" core of [desloppify](https://github.com/peteromallet/desloppify). Scans your codebase, scores it 0-100, and tells your AI agent (or you) what to fix next.

This is **v0.8 in progress**: Rust + TypeScript/JavaScript + Go + Python + C + C++ language support plus React, Next.js, and Nuxt framework-aware detectors that auto-activate on the right projects, composite score 0-100, persistent state. See [docs/superpowers/specs/2026-05-01-rust-port-design.md](docs/superpowers/specs/2026-05-01-rust-port-design.md) for the full design and what's deliberately deferred.

## Install

```bash
git clone <this-repo> noAIslop
cd noAIslop
cargo install --path dslp-cli
```

After install, the binary is `dslp`.

## Usage

```bash
# Scan a project (writes state to .dslp/state.json)
dslp scan --path /path/to/project

# Show the highest-priority open issue
dslp next --path /path/to/project

# Print the score (no rescan)
dslp score --path /path/to/project

# List all open issues in priority order
dslp findings --path /path/to/project
dslp findings --limit 10 --path /path/to/project
```

Every command supports `--json` for machine-readable output suitable for piping into an agent:

```bash
dslp scan --json --path .
dslp next --json --path .   # full Issue payload, with fingerprint
dslp score --json --path .  # composite + per-detector breakdown
dslp findings --json --path .   # array of all unresolved Issues
```

Add `.dslp/` to your `.gitignore`. State persists across runs so progress carries over.

## What it detects

### Rust (`.rs`)

| Detector | What it flags |
|---|---|
| `rust.file-size` | Files over 400 lines (configurable). Severity bumps to High at 600. |
| `rust.complexity` | Functions with cyclomatic complexity ≥ 10 (counts `if`, `match` arms, loops, `?`, `&&`, `\|\|`). |
| `rust.length` | Functions ≥ 50 lines. Low severity. |
| `rust.dead-pub-fn` / `-struct` / `-enum` / `-trait` | Public items whose name isn't referenced anywhere in the corpus. Low severity (heuristic — false positives possible on macro-generated or trait-dispatched code). |

### TypeScript / JavaScript (`.ts`, `.tsx`, `.js`, `.jsx`, `.cjs`, `.mjs`)

| Detector | What it flags |
|---|---|
| `ts.file-size` | Files over the threshold (same configurable thresholds as Rust). |
| `ts.complexity` | Functions / arrow functions / methods with cyclomatic complexity ≥ 10. Counts `if`, `for`, `while`, `do`, `switch case`, `?:`, `catch`, `&&`, `\|\|`, `??`. |
| `ts.length` | Functions ≥ 50 lines. Low severity. |

A "dead-export" detector for TS/JS is intentionally not in this slice — the TS module system (default exports, re-exports, dynamic imports, JSX references) needs its own design pass.

### Go (`.go`)

| Detector | What it flags |
|---|---|
| `go.file-size` | Files over the threshold. |
| `go.complexity` | Functions and methods with cyclomatic complexity ≥ 10. Counts `if`, `for`, switch `expression_case` / `default_case`, `select` `communication_case`, type-switch `type_case`, plus `&&` / `\|\|`. |
| `go.length` | Functions ≥ 50 lines. Low severity. |

### Python (`.py`, `.pyi`)

| Detector | What it flags |
|---|---|
| `python.file-size` | Files over the threshold. |
| `python.complexity` | Functions / methods (including nested defs) with cyclomatic complexity ≥ 10. Counts `if`, `elif`, `for`, `while`, `except`, `match` `case`, conditional expressions (`x if c else y`), plus `and` / `or`. |
| `python.length` | Functions ≥ 50 lines. Low severity. |

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

## Configuration

Drop a `.dslp/config.toml` in your project root:

```toml
[scan]
exclude = ["target/", "node_modules/", "vendor/"]

[detectors.file_size]
threshold = 400
severity_at = 600

[detectors.complexity]
cyclomatic_threshold = 10
length_threshold = 50

[detectors.dead_pub]
ignore_paths = ["src/lib.rs"]
```

Every key is optional; missing values fall back to defaults.

## Scoring

```
penalty = sum of severity weights (Low=1, Medium=3, High=8, Critical=20)
size    = max(1, total_loc / 1000)
score   = 100 * exp(-penalty / (size * 25))
```

A single composite 0-100 score, computed from open (unresolved) issues. Larger codebases tolerate more penalty before the score drops. Subjective LLM dimensions are still deferred to a later slice.

## Architecture

Cargo workspace:

- `dslp-core` — domain types (`Issue`, `Finding`, `Score`, `LanguagePlugin` trait). No I/O, no parsers.
- `dslp-base` — file discovery (gitignore-aware), config loading, tree-sitter glue.
- `dslp-engine` — scan/score/queue/state pipeline.
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
9. Later: workflow loop (`resolve`/`suppress`, anti-gaming score split); LLM-backed subjective review; plan/triage workflow; scorecard badge; `--lang` monorepo support; autofix.

## Performance

Release-mode scan of this very workspace completes in well under 100ms end-to-end on a Linux laptop. The hot path is single-pass tree-sitter parsing per file with no ASTs round-tripped to disk.

```bash
cargo build --release --bin dslp
time ./target/release/dslp scan --path .
```

## Testing

```bash
cargo test --workspace
```

v0.8 added C/C++ parser glue and the `c.*`/`cpp.*` detectors with shared declarator-identifier extraction.

## License

Same dual MIT-or-Apache-2 as is conventional for Rust workspaces. See `LICENSE` (TODO: drop one in).
