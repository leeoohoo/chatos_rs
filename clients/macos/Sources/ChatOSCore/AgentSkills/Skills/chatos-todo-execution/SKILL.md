---
name: chatos-todo-execution
description: Execute the one Todo bound to the current ChatOS executor run, record evidence-backed progress, inspect prior progress, and finish as completed or blocked without changing task identity.
---

# Todo execution

Start with `todo_get_context`. The delivery fixes the Todo, assignee, team, project, source messages, and approved capability plan; tool arguments cannot switch them.

Record meaningful milestones with `todo_progress_append`: action taken, observed result, and next step. Use `todo_read_progress` when resuming or coordinating with earlier execution evidence.

Call `todo_complete` only when the requested outputs and acceptance criteria are actually satisfied. Include verifiable results, not plans or optimistic language. Call `todo_block` when progress cannot continue safely, preserving the reason and execution state needed by the manager or Human.

Read [references/evidence-and-terminal-states.md](references/evidence-and-terminal-states.md) for progress cadence, completion evidence, long-lived asset suggestions, blocking, cancellation, and resumed Runs.
