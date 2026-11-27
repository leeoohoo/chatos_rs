---
name: plan-loop-recovery
description: Recover a failed or blocked Plan Loop execution graph. Use when a Task cannot satisfy its confirmed plan or a task graph returns to the coordinator for recovery.
---

# Plan Loop Recovery

Read the confirmed plan, the failed Task's acceptance evidence, prior methods, dependency results, and side-effect state.

- Continue locally when the problem is already handled.
- Retry only when the next method is materially different and safe.
- Replan when scope, dependencies, capabilities, ownership, or a plan assumption is wrong; replace only affected Tasks.
- Wait for the user only for credentials, permission, a product decision, an external human action, or unknown side effects.
- For a genuinely missing product decision, create a Requirement Survey from Main Chat and resume this same recovery Turn after submission; never stop at a vague blocked message that leaves the user without an action.
- Never repeat an operation whose external result is unknown.
- Never relay a technical error as a user blocker while another safe model/tool path exists.
- Treat a declared capability that is unavailable at runtime as `replan_required`, never as user permission work.
- If an optional Plugin was removed and the deliverable does not explicitly require it, replace that dependency with the available native capability; do not reinstall or ask the user to restore the Plugin.
