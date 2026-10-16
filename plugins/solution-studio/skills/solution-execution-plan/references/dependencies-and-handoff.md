# Dependency scope and handoff

## Plan-density checks

Reviewability is a correctness property, not cosmetic polish. Before upserting a plan, calculate at least:

- total tasks versus the number of user-facing requirements;
- direct task count for each requirement;
- how many requirements each task claims to deliver;
- direct prerequisites per task and the longest obvious prerequisite chain;
- milestone, review, or “decision” nodes that do not produce an independently useful output.

Treat these as refactoring signals rather than quotas. A typical bounded feature or two-module rebuild should usually fit in a handful of outcome stages and roughly a dozen to twenty tasks. A requirement with more than about eight direct tasks, a task linked to more than three requirements, or a plan growing past roughly three tasks per requirement deserves an explicit second pass. Keep the larger form only when the user's scope genuinely requires independently assignable and independently accepted work; state that reason in the task descriptions. Otherwise consolidate mechanical steps, create meaningful child requirements, or move shared foundations out of business-requirement mappings.

Do not turn unresolved details into one task per question. Group related product or contract decisions into one reviewable scope-and-contract milestone when they share the same approver and gate. Do not create a stage-exit task merely to restate that every task in a stage must finish; dependencies already express that condition unless a human approval or concrete integration artifact exists.

## Cross-requirement tasks

Use one stable `T-*` ID for one task in the workspace. Link only requirements whose acceptance criteria that task directly delivers or verifies; do not copy the task into each requirement and do not attach unrelated requirements merely because they consume a shared foundation. `dependsOn` references the workspace-wide ID, including prerequisites owned by another requirement. The per-requirement canvas is only a view of the same plan.

If a user cannot find a prerequisite, read the workspace, locate its title and `requirementIds`, and explain where it belongs. Distinguish a hidden cross-requirement task from an unknown ID; an unknown reference requires repair, never silent removal. When showing the full chain, follow transitive prerequisites with a visited set and preserve the original edges.

## Real parallelism

For each proposed parallel task, identify its expected outputs and possible write targets in its description and deliverables. If two tasks can modify the same source file, document, design asset, shared configuration, schema migration, or generated output, default to serial writes or one owner. Read-only investigation may run in parallel with a clear evidence scope. Separate worktrees alone do not prove the final changes can be integrated safely.

Express ordering through `dependsOn`, not numbering, phases, layout coordinates, or prose alone. State the output or decision that the prerequisite contributes in the task description. Do not add a universal chain when work is demonstrably independent. A single task still needs a concrete expected output and acceptance criteria; no justification for not splitting is required.

## Planning versus execution

The authoring component cannot update task status. Do not mark a task done because its plan was written or a model response ended. When implementation is separately authorized, select the “任务执行器” component and hand off the exact saved workspace revision, task ID, requirement/design references, expected outputs, acceptance criteria, prerequisite results, and write boundaries. Claim the task there before making implementation changes.

Do not dispatch dependent work before prerequisite evidence is available. If a local execution mapping is not supported, say that execution status is not linked, rather than presenting planned statuses as live runner events. Preserve unrelated planning content and existing task IDs when revising the handoff.
