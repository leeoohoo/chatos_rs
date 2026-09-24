---
name: solution-validation
description: Validate a Solution Studio workspace for completeness, traceability, revision freshness, and dependency-DAG integrity. Use before review, approval, export, handoff, or when a plan appears blocked or inconsistent.
metadata:
  chatos.role: leaf
  chatos.activation-policy: model-or-user
  chatos.context-mode: inline
---

# Solution Validation

Call `solution_validate` on the target workspace and interpret the result rather than reimplementing graph validation from prose. Validation is diagnostic; `solution_finalize` is the delivery gate that re-reads the exact revision, verifies request-specific design block IDs, registers the internal artifact, and emits the completion proof required by Task Runner.

Treat unknown references, self-dependencies, cycles, missing required documents, empty or non-rendering SVG previews, and tasks without acceptance criteria as blocking. Also check whether design and plan revision links are stale, whether the project has a technical baseline and overall architecture, whether every requirement reaches exactly one substantive design section and at least one task, and whether unresolved questions or risks need human review. For development projects, a sentence-length summary without module, interface/data, flow, failure, and verification detail is not a completed design.

Report the topological order and `readyTaskIds` when the user asks what can start. `ready` for the overall workspace means structural checks pass and all three documents are approved; it does not mean every task is complete.

Fix only planning artifacts the user authorized you to revise. Do not mark documents approved, resolve business questions, or mark implementation tasks done without evidence. Export only after validation, and state any non-blocking warnings that remain. In the final report, distinguish “planning content was written” from “the plan is approved” and “implementation tasks are done.” Never announce the work simply as completed while documents remain draft, blocking validation issues remain, or saved visuals have not been visibly rendered and inspected; report the actual document statuses, counts, and verification performed.
