---
description: Scan the project with dslp and summarise the score and top issues
argument-hint: "[path]"
allowed-tools: Bash(dslp:*)
---

Project path: `$ARGUMENTS` (use `.` if empty).

Run `dslp scan --json --path <path>`, then `dslp plan --json --limit 5 --path <path>`.

Report the lenient and strict score, what changed since the last scan, and the five files with the most to fix. Don't change any code.
