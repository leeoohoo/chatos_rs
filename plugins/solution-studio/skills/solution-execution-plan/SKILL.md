---
name: solution-execution-plan
description: Turn a solution design into an executable task DAG with explicit prerequisites, deliverables, acceptance criteria, and requirement-to-design traceability. Use when a user asks for an implementation roadmap, task breakdown, dependency plan, critical ordering, or ready-to-start work.
metadata:
  chatos.role: leaf
  chatos.activation-policy: model-or-user
  chatos.context-mode: inline
---

# Solution Execution Plan

Create a plan whose dependency graph is correct without relying on list order or prose.

Read the current design and set `basedOnDesignRevision` to the revision used. Break work into reviewable tasks with one primary outcome. For every task, provide a stable ID, phase, description, concrete deliverables, acceptance criteria, requirement links, and design-section links.

Use `dependsOn` only for real prerequisites: a task belongs there when its output or decision is needed before the dependent task can start. Avoid phase-wide blanket dependencies and false serial ordering. Add explicit review or milestone tasks when an approval gates downstream work. Independent tasks should remain parallel.

Before calling `solution_upsert_execution_plan`, verify that all dependency IDs exist, no task depends on itself, and the graph is acyclic. Canvas positions are optional presentation state and never define order. Planned tasks with every prerequisite complete are computed as ready; do not persist a separate ready status. After the upsert, call `solution_finalize` with `scope: "plan"`, the returned workspace revision, and the visual block inventory already promised by the design; the plan is not delivered while the completion requirement remains pending.

Read [the workspace schema](../solution-studio/references/workspace-schema.md) for the exact shape. This skill plans work and may update task status when asked; it does not execute the planned tasks.
