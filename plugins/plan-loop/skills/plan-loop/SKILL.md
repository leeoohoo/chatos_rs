---
name: plan-loop
description: Plan-first orchestration for requests that require Tasks. Use while Plan Loop is enabled to investigate requirements before execution, bind work to the confirmed plan, and keep technical blockers moving without unnecessary user intervention.
---

# Plan Loop

Before Main Chat finishes, call `decide_task_route` exactly once. Choose `direct_response` only when the request can be fully answered without durable Task work; choose `durable_task` for plans, project investigation, files, commands, implementation, Plugins, or tracked execution. Decide from the requested outcome, never by matching words or phrases. Follow the Host-provided `next_action`.

For a request routed to `direct_response`, answer it normally.

For any request that needs files, tools, changes, external research, or durable Tasks:

1. Before creating execution work, create exactly one Task with `task_kind: planning`.
2. Give it a concrete research objective, the user's requirements, evidence-based outputs, and acceptance criteria. The Host adds project-read and Requirement Survey capabilities.
   Use those native capabilities and `requirement_plan_save` as the planning record. Do not attach an optional authoring or persistence Plugin to the planning Task unless the user explicitly selected that Plugin as part of the requested deliverable.
3. The planning Task reads the project and authorized sources. It uses Requirement Survey only when a missing human choice materially changes scope, safety, or acceptance; technical errors are not user questions.
4. The planning Task publishes the complete proposal with `requirement_plan_save` and finishes with that same proposal.
5. Do not ask the user to return to chat after confirmation. The Host resumes the original coordinator with the confirmed immutable plan.
6. Create execution Tasks only from that Host-bound plan. Preserve its requirements, task boundaries, dependencies, expected outputs, and acceptance criteria.

When execution evidence invalidates the plan, revise the plan and replace only affected work. Keep valid completed artifacts and unaffected Tasks.

Treat a blocker as requiring the user only when it needs credentials, permission, a product decision, an external human action, or review of unknown external side effects. Planning Tasks collect product decisions through Requirement Survey and never fall back to generic Ask User forms. During Main Chat recovery, collect a genuinely missing product decision with Requirement Survey and resume the same Turn; do not end with a vague blocked message. If a declared Task capability is unavailable or inconsistent at runtime, classify it as `replan_required`; do not ask the user to configure ChatOS. If an optional Plugin was removed and the confirmed deliverable does not require it, replan onto the native capabilities instead of restoring the Plugin. Otherwise repair it, use a different safe method, or replan.
