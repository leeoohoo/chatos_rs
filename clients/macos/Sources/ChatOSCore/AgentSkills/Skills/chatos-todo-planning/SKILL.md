---
name: chatos-todo-planning
description: Plan and schedule shared ChatOS team Todos, including assignees, dependencies, execution capabilities, priority, updates, reordering, and starting the next ready task.
---

# Todo planning and scheduling

Use the shared board as the source of truth. Before creating or changing work, read the current Todos and obtain fresh execution or dependency options.

A Todo must state its objective, scope, expected outputs, acceptance criteria, constraints, source messages, assignee, and only the capabilities actually needed. Remote repository clone, build, run, or analysis work belongs in a Todo with terminal capability; it is not a project-team creation request.

Scope each Todo to work its assignee can actually own and complete with the selected capabilities. Split work at responsibility boundaries: for example, technical verification and a real-person playtest are separate Todos with different owners. Do not bundle an Agent-deliverable with a later Human, manager, approval, deployment, or cross-role activity and then mark the Agent blocked on that later activity.

Dependencies must stay within one team and represent real prerequisites. Reordering does not override dependencies. `todo_start_next` atomically selects the current Agent's highest-priority ready task; never choose an internal Todo ID yourself.

Read [references/planning-and-recovery.md](references/planning-and-recovery.md) for capability selection, dependency graphs, reprioritization, cancellation, `needsReview`, and positive/negative examples.
