---
name: solution-studio
description: Turn an existing codebase or a greenfield product idea into traceable requirements, a solution design, and a dependency-aware execution plan. Use for end-to-end planning requests; do not use it to execute implementation tasks.
metadata:
  chatos.role: router
  chatos.activation-policy: model-or-user
  chatos.context-mode: inline
---

# Solution Studio

This Skill belongs to the independently selectable “方案创建器” component. Build the active ChatOS project's single coherent plan whose functional modules, requirements, design sections, and tasks can be traced to one another. Never claim or complete execution tasks from this component.

## Route the work

1. Call `solution_get_active_context`. If `workspace` exists, update that project plan; otherwise create it through the first upsert. Never create a second plan for the same ChatOS project.
2. Use `solution-discovery` to establish the project profile, functional modules, and requirements. For an existing project, inspect relevant code and documentation before drafting; for a greenfield effort, separate user-stated facts from assumptions and open questions.
3. Use `solution-design` only after the requirement set is coherent enough to design against. Write the project-wide technical baseline and architecture first, then complete the detailed design module by module.
4. Use `solution-execution-plan` to derive module-owned tasks and the complete cross-module `dependsOn` DAG from the approved direction. Remove redundant transitive edges before saving.
5. Use `solution-validation` before presenting the result. Resolve blocking structural issues; surface unresolved product questions instead of inventing answers. A design mutation intentionally makes the execution plan stale. Treat `stale_plan` as an in-scope repair signal: re-read the current design, upsert the execution plan with `basedOnDesignRevision` set to the latest design revision, then validate again. Do not stop or report a blocker for `stale_plan` while the authorized planning artifacts can still be updated.
6. Call `solution_finalize` after the last mutation, using the current workspace revision, the actual delivery scope, and every visual block promised in this request. This re-reads the canonical workspace, verifies the named blocks, and registers the internal Markdown artifact. A planning mutation leaves a pending completion requirement; do not report success until finalization returns the matching completion proof.
7. Report the saved artifact honestly: list the document statuses and concrete requirement/design/task counts, and say whether SVG previews were actually rendered. A generation turn finishing is not the same as the project plan being approved or its execution tasks being completed.

The Skills define the happy path; the completion proof is only a guard against accidental false success. Do not aim merely to satisfy validation or produce an acyclic graph. Aim to leave the user with a readable, substantive plan whose promised content is already visible inside Solution Studio. Before finalizing, inspect the execution plan per requirement: shared technical foundations must not be mapped onto every business requirement, and a requirement must not open into a wall of mechanical subtasks or synthetic stage gates.

## Intermediate validation is a work list

Successful requirement upserts can return `validation.valid: false` with `design_empty`, `plan_empty`, or requirement-without-design/task issues. These describe documents you have not written yet; they are not tool failures, missing capabilities, or reasons to end the Task. For an end-to-end request, continue in this same Run: activate `solution-design`, read its schema, save the substantive design; activate `solution-execution-plan`, save all requested tasks and dependencies; then validate, repair, and finalize. A Skill activation alone does not create its document. An unfinished draft or the absence of later-stage content is not a blocker. If a tool genuinely fails, correct its explicit diagnostics within the authorized scope before reporting the precise unresolved failure.

Read [references/workspace-schema.md](references/workspace-schema.md) before writing a complete workspace document.

## Invariants

- One ChatOS project has exactly one project profile, one total requirements document, one overall design, and one execution plan. Every planning request updates that plan; no caller-controlled key can create a parallel workspace.
- The workspace schema version is exactly `2`. Do not translate, repair, or preserve legacy workspace shapes; invalid old data is discarded by the service.
- Treat `scope.projectId`, `scope.projectName`, `scope.connectorWorkspaceId`, and `scope.projectRoot` as host-supplied context. Never ask the user to enter them, invent them, or add them to upsert arguments. The service binds newly created workspaces to the active ChatOS project.
- Do not confuse the Solution Studio `workspaceId` with `scope.connectorWorkspaceId`: the former identifies the project's single planning document; the latter identifies the ChatOS connector workspace carrying the project root.
- Use stable IDs: `E-*` for evidence, `MOD-*` for functional modules, `R-*` for requirements, `D-000-B-*` for project-level design blocks, `D-*` for requirement designs, `D-*-B-*` for typed design blocks, `ADR-*` for decisions, and `T-*` for tasks.
- Every requirement, design section, and task has an owning `moduleId`. Parent and child requirements stay in the same module. Modules describe functional ownership, never schedule phases.
- The requirements document contains the project background and total requirements before its structured child requirements.
- For development projects, the design document contains one project-level technical baseline and overall architecture. Non-development plans need a project-level written approach but must not add a decorative software architecture diagram. Each requirement owns exactly one detailed design section and one execution-plan slice. Alternatives belong in design decisions, not parallel solution sections.
- Treat `tasks[].dependsOn` as the only source of execution order. Never encode order only in prose or canvas positions, and never store a direct dependency already implied by another path.
- Every new task starts as `planned`. Planning tools may create or revise only planned task definitions; status transitions and `statusEvents` are owned exclusively by the “任务执行器” component.
- Do not claim that plan generation authorizes implementation. A plan records work; it does not perform it.
- Keep the plugin self-contained: design SVG and the execution DAG are rendered only from this workspace's own data, with no cross-plugin references or dependencies.
- While this Skill is active, treat the host project directory as a read-only evidence source. Do not create planning `.md`, `.svg`, `.json`, screenshots, or other Solution Studio deliverables under `scope.projectRoot`. Store planning text and SVG code only through Solution Studio tools. Temporary visual-QA files may exist only outside the project directory and are not deliverables.
- A successful file write, terminal command, or model response is not Solution Studio delivery evidence. Only saved workspace content plus a matching `solution_finalize` completion proof closes the planning turn.
