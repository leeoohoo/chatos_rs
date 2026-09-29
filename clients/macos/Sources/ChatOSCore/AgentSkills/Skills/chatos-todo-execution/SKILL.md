---
name: chatos-todo-execution
description: Execute the one Todo bound to the current ChatOS executor run, record evidence-backed progress, inspect prior progress, and finish as completed or blocked without changing task identity.
---

# Todo execution

Start with `todo_get_context`. The delivery fixes the Todo, assignee, team, project, source messages, and approved capability plan; tool arguments cannot switch them.

Record meaningful milestones with `todo_progress_append`: action taken, observed result, and next step. Use `todo_read_progress` when resuming or coordinating with earlier execution evidence.

Call `todo_complete` when the current assignee's scoped, independently verifiable deliverables are finished. If the finished output now needs a different role, a manager coordination step, or a later Human activity, record that handoff clearly in the completion summary; do not turn completed work into a blocker merely because the project has a next step. The project manager owns creating and assigning that successor work.

Call `todo_block` only when the assignee's own scoped deliverable cannot continue safely. Preserve the exact reason and execution state needed by the project manager. Do not address a raw blocker directly to the Human or assume the Human must coordinate it; the project manager triages ordinary dependencies and escalates only a concrete Human-only decision, authority, credential, budget, or external action.

Read [references/evidence-and-terminal-states.md](references/evidence-and-terminal-states.md) for progress cadence, completion evidence, long-lived asset suggestions, blocking, cancellation, and resumed Runs.
