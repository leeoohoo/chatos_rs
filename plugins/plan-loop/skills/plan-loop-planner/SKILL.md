---
name: plan-loop-planner
description: Investigate a Task-bound request and publish the requirement-aware execution plan before implementation begins. Use for task_kind=planning work while Plan Loop is active.
---

# Plan Loop Planner

Read the user's request, authorized project evidence, attachments, and relevant external sources before proposing execution work. Preserve stated requirements and distinguish verified facts from assumptions.

The Host-provided Requirement Survey and `requirement_plan_save` are the authoritative planning path. Do not require Solution Studio or any other optional authoring/persistence Plugin. Use a Plugin in planning only when the user explicitly selected that Plugin as part of the requested deliverable; a previously installed or later removed Plugin is not an implicit dependency.

Use the Host-provided Requirement Survey capability when a missing human choice materially changes scope, safety, ownership, or acceptance. After submitted answers resume this same Run, incorporate them instead of starting another planning Task.

Never replace Requirement Survey with a generic Ask User form. If the declared survey capability cannot activate or save the plan, report `replan_required` so Main Chat can repair the Task contract; do not ask the user to configure ChatOS capabilities.

Produce one concrete proposal containing:

- goal, scope, exclusions, constraints, and assumptions;
- evidence and important tradeoffs;
- execution Tasks, dependencies, owned outputs, and acceptance checks;
- risks and the recovery boundary between autonomous work and genuine user action.

Publish the complete proposal with `requirement_plan_save`, passing only `expected_revision` and `markdown`. Never supply or guess `survey_id`, `plan_id`, `run_id`, owner, project, or another internal identifier; the Host binds them from this Run, including after intermediate research tools. Do not create execution Tasks, infer confirmation, or ask the user to send another chat message. The Host resumes the coordinator after the user confirms the exact plan revision.
