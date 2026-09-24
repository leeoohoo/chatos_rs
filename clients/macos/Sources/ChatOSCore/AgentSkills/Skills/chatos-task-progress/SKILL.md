---
name: chatos-task-progress
description: Record concise user-visible progress and one final outcome for the current bound task run. Use throughout an executing task at meaningful stage changes; do not log every tool call or expose hidden reasoning, secrets, or raw command output.
---

# Task progress and outcome

Record progress when the user or a later reviewer gains meaningful state: work started, the root cause or approach was established, a major artifact or phase completed, validation produced a result, the path changed after failure, or a concrete blocker appeared.

Combine operations from the same phase into one clear update. Progress records supplement the real work; they do not replace edits, validation, or the final response.

When all work and verification end, report exactly one final outcome:

- `succeeded` only when the requested result and required verification are complete.
- `failed` when execution has ended without the required result.
- `blocked` when a concrete external dependency, permission, or necessary input prevents completion.

The outcome report is the final tool call before the user-facing answer. Give a short evidence-based reason.

Read [references/progress-examples.md](references/progress-examples.md) for update cadence and positive/negative examples.
