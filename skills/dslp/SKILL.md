---
name: dslp
description: Use when asked to clean up, de-slop, audit or score the quality of a codebase, find what to refactor next, or work through dslp findings. Covers Rust, TypeScript/JavaScript, Go, Python, C and C++ (plus React, Next.js, Nuxt).
---

# dslp: scan, score, fix

`dslp` analyses a project, tracks what it finds as *issues* in `.dslp/state.json`, and keeps a 0-100 score. Issues persist between scans, so work the queue one issue at a time.

Every command takes `--path <dir>` (default: current directory). Use `--json` on everything except `init` and `badge` and parse the output instead of the human text.

## Fix loop

1. If `.dslp/` doesn't exist, run `dslp init` once.
2. `dslp scan --json` to analyse and get the score, then `dslp findings --json` for every open issue. Fields: `fingerprint`, `detector`, `severity`, `file`, `span`, `message`, `fix_hint`.
3. Print **all** open issues as a numbered list (short fingerprint, detector, severity, `file:line`, message), plus the score. Then ask the user which ones to fix. Do not edit any code before they answer.

   If the `AskUserQuestion` tool is available, ask with it so each issue is a checkbox: one call, up to 4 questions with `multiSelect: true`, each holding 2-4 issues, so 16 per call. Split the list in order (highest priority first) and name each question by its range in `header` (≤ 12 chars, e.g. `#1-4`). Each option's `label` is `#<n> <detector>`; its `description` is `<severity> · <file>:<line> · <message>`. With a single open issue, ask one question with the options `Fix it` and `Skip`. If more than 16 are open, say in the question text that the rest can be picked by number in "Other" (e.g. `17, 20-23`, or `all`). Without the tool, ask in plain text.
4. For each chosen issue, read the code at `file`/`span` and fix the underlying problem, using `fix_hint` as a guide. Keep behaviour the same; run the project's tests if there are any.
5. `dslp resolve <fingerprint>` for each fixed issue, then `dslp scan --json`. A resolve is optimistic: the scan reopens the issue if it is still there.
6. If any chosen issue is still open, fix it again and go back to step 5. Repeat until none of the chosen issues show up in `dslp findings --json`.
7. Explain each fix: what the problem was and what you changed. Give the score before and after.
8. List the remaining open issues and ask whether to continue with them. If yes, go back to step 3.

For a wider view: `dslp plan --json` (open issues grouped by file, worst first), `dslp score --json` (per-detector breakdown).

## Dismissing an issue

Only when the code is deliberate and fixing it would make things worse:

- `dslp suppress <fingerprint> --reason "<why>"`, or
- a `dslp-ignore <detector-id>: <reason>` comment on the line of the issue or the line above.

Both make it a *wontfix*, which still lowers the **strict** score. Never suppress just to raise the number; say which issues you suppressed and why. `dslp reopen <fingerprint>` undoes it.

## Exit codes

- `0` success
- `1` a check said no: gate failed (`--fail-under` / `--fail-on-new`), no scan yet, or `next` has no open issues
- `2` error or bad usage

## Useful options

- `--lang <go|typescript|rust|...>` scores one language on its own in a monorepo.
- `dslp scan --since <git-rev>` reports only on files changed since that revision.
- Detectors and thresholds are configured in `.dslp/config.toml`.
