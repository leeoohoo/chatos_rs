---
name: chatos-async-task-orchestration
description: Arrange durable background work through the bound Task Runner tools, including finding related history, creating scoped tasks with minimal capabilities, cancelling obsolete work, and handing off once. Use when work must continue against real project or external resources beyond the current response.
---

# Asynchronous task orchestration

Use task history as context, not as proof that the current request has been completed. Create new work for a new execution request; cancel still-active work when the user's latest intent replaces it.

Each created task must have a concrete objective, acceptance criteria, necessary input, and only the capabilities needed to finish. The platform binds user, project, workspace, callback, and execution routing; do not ask for or invent their internal identifiers.

After arranging the requested work, call `wait_for_task_completion` exactly once as the handoff signal. When it succeeds, call no more task tools in that turn and immediately give a concise user-facing statement that work has started. Do not expose internal task-routing terminology.

Read [references/capabilities-lifecycle-and-recovery.md](references/capabilities-lifecycle-and-recovery.md) before selecting built-in capabilities, creating dependent tasks, handling superseded work, or recovering from unavailable project capabilities.
