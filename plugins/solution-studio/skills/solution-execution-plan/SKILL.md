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

Read the current design and set `basedOnDesignRevision` to the revision used. Plan one functional module at a time while checking the complete cross-module DAG after every module. Break work into reviewable tasks with one primary outcome. For every task, provide a stable ID, owning `moduleId`, phase, description, concrete deliverables, acceptance criteria, requirement links, and design-section links. New tasks must always start as `planned`; the authoring capability never claims tasks or writes execution status.

## Keep the plan reviewable

Size the plan to the user's requested delivery, not to every fact mentioned in the design. Prefer a small number of outcome-oriented stages and tasks that a human can review, assign, and accept. Consolidate adjacent setup, implementation, and verification steps when they produce one reviewable outcome; split only when ownership, sequencing, risk, or acceptance genuinely differs. Do not manufacture a long “phase 0” from every open question, and do not add milestone or stage-exit nodes unless an actual approval gates downstream work.

Link a task to a requirement only when that task directly delivers or verifies one of the requirement's acceptance criteria. Shared architecture, security, observability, API conventions, or data foundations belong in a project-level foundation task or milestone; their influence on later work is expressed through `dependsOn`, not by attaching every business requirement ID to every technical task. A multi-requirement task is appropriate only when its single deliverable is jointly accepted by those requirements.

Before saving, inspect the plan from each requirement's perspective. If one requirement expands into a wall of tasks or most tasks repeat the same requirement IDs, stop and refactor: merge mechanical subtasks, move shared work to a project-level task, or decompose an actually broad requirement into meaningful child requirements. Use the plan-density checks in [references/dependencies-and-handoff.md](references/dependencies-and-handoff.md); exceeding them requires a concrete reason visible in the plan rather than silently emitting a large graph.

Use `dependsOn` only for real prerequisites: a task belongs there when its output or decision is needed before the dependent task can start. Avoid phase-wide blanket dependencies and false serial ordering. Store the transitive reduction: if `T-003` already depends on `T-002` and `T-002` depends on `T-001`, do not also add `T-001` directly to `T-003`. Add explicit review or milestone tasks only when an approval gates downstream work. Independent tasks should remain parallel.

Task IDs are workspace-wide, not requirement-local. A task may support several requirements, and a prerequisite may belong to another requirement. Never remove that dependency to simplify a requirement's diagram. Report ready-to-start work using `solution_validate.readyTaskIds` from the complete workspace; a filtered canvas is not an execution source. “Ready” here describes dependency conditions, not authorization, approval, dispatch, or a running local Task.

When tasks may write the same files or shared artifact, prefer one owning task or an explicit prerequisite that serializes their writes. Parallelize only with clear non-overlapping write boundaries; do not infer independence merely from different requirement IDs. For plan-density checks, cross-requirement dependencies, uncertain write boundaries, or implementation handoff, read [references/dependencies-and-handoff.md](references/dependencies-and-handoff.md).

Before calling `solution_upsert_execution_plan`, verify that all dependency IDs exist, no task depends on itself, and the graph is acyclic. Canvas positions are optional presentation state and never define order. Planned tasks with every prerequisite complete are computed as ready; do not persist a separate ready status. After the upsert, call `solution_finalize` with `scope: "plan"`, the returned workspace revision, and the visual block inventory already promised by the design; the plan is not delivered while the completion requirement remains pending.

Every authorized design mutation invalidates the previous plan revision by design. If validation reports `stale_plan`, re-read the current workspace and immediately upsert a substantively equivalent or appropriately revised execution plan whose `basedOnDesignRevision` equals the current `design.revision`. Then validate again. `stale_plan` is not a reason to stop unless the upsert itself fails or repairing the plan requires a product decision the user has not supplied.

Read the `references/workspace-schema.md` resource of the already required `solution-studio` router for the exact shape. With the Host Skill protocol, use that router's catalog-provided `skill_ref` and declared resource path; do not pass a sibling directory or `../` traversal as `relative_path`. This skill defines work and dependencies only. Task claiming, blocking, completion, cancellation, and all status history belong exclusively to the separately selected “任务执行器” component.
