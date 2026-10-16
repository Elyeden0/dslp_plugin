---
description: Scan with dslp, choose which issues to fix, and fix them
argument-hint: "[path]"
---

Project path: `$ARGUMENTS` (use `.` if empty).

Follow the dslp skill's fix loop on that project: list every open issue, ask which to fix, fix and rescan until the chosen ones are gone, explain the fixes, then ask whether to continue with the rest. List any issue you suppressed instead of fixing, with the reason.
