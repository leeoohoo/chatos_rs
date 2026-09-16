---
name: solution-studio
description: Turn an existing codebase or a greenfield product idea into traceable requirements, a solution design, and a dependency-aware execution plan. Use for end-to-end planning requests; do not use it to execute implementation tasks.
---

# Solution Studio

Build one coherent planning workspace whose requirements, design sections, and tasks can be traced to one another.

## Route the work

1. Call `solution_get_active_context`, then reuse the requested workspace or choose one stable `artifactKey` for the planning effort.
2. Use `solution-discovery` to establish requirements. For an existing project, inspect relevant code and documentation before drafting; for a greenfield effort, separate user-stated facts from assumptions and open questions.
3. Use `solution-design` only after the requirement set is coherent enough to design against.
4. Use `solution-execution-plan` to derive tasks and explicit `dependsOn` relationships from the approved direction.
5. Use `solution-validation` before presenting the result. Resolve blocking structural issues; surface unresolved product questions instead of inventing answers.

Read [references/workspace-schema.md](references/workspace-schema.md) before writing a complete workspace document.

## Invariants

- Preserve the same `artifactKey`, `title`, and `sourceMode` across the three upsert calls.
- Treat `scope.projectId`, `scope.projectName`, `scope.connectorWorkspaceId`, and `scope.projectRoot` as host-supplied context. Never ask the user to enter them, invent them, or add them to upsert arguments. The service binds newly created workspaces to the active ChatOS project.
- Do not confuse a Solution Studio `workspaceId` with `scope.connectorWorkspaceId`: the former identifies one planning document; the latter identifies the ChatOS connector workspace carrying the project root.
- Use stable IDs: `E-*` for evidence, `R-*` for requirements, `D-000-B-*` for project-level design blocks, `D-*` for requirement designs, `D-*-B-*` for typed design blocks, `ADR-*` for decisions, and `T-*` for tasks.
- The requirements document contains the project background and total requirements before its structured child requirements.
- The design document contains one project-level technical baseline and overall architecture. Each requirement then owns exactly one detailed design section and one execution-plan slice. Alternatives belong in design decisions, not parallel solution sections.
- Treat `tasks[].dependsOn` as the only source of execution order. Never encode order only in prose or canvas positions.
- Do not claim that plan generation authorizes implementation. A plan records work; it does not perform it.
- Keep the plugin self-contained: design SVG and the execution DAG are rendered only from this workspace's own data, with no cross-plugin references or dependencies.
