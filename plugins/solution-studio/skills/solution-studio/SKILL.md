---
name: solution-studio
description: Turn an existing codebase or a greenfield product idea into traceable requirements, a solution design, and a dependency-aware execution plan. Use for end-to-end planning requests; do not use it to execute implementation tasks.
metadata:
  chatos.role: router
  chatos.activation-policy: model-or-user
  chatos.context-mode: inline
---

# Solution Studio

Build the active ChatOS project's single coherent plan whose requirements, design sections, and tasks can be traced to one another.

## Route the work

1. Call `solution_get_active_context`. If `workspace` exists, update that project plan; otherwise create it through the first upsert. Never create a second plan for the same ChatOS project.
2. Use `solution-discovery` to establish requirements. For an existing project, inspect relevant code and documentation before drafting; for a greenfield effort, separate user-stated facts from assumptions and open questions.
3. Use `solution-design` only after the requirement set is coherent enough to design against.
4. Use `solution-execution-plan` to derive tasks and explicit `dependsOn` relationships from the approved direction.
5. Use `solution-validation` before presenting the result. Resolve blocking structural issues; surface unresolved product questions instead of inventing answers.
6. Call `solution_finalize` after the last mutation, using the current workspace revision, the actual delivery scope, and every visual block promised in this request. This re-reads the canonical workspace, verifies the named blocks, and registers the internal Markdown artifact. A planning mutation leaves a pending completion requirement; do not report success until finalization returns the matching completion proof.
7. Report the saved artifact honestly: list the document statuses and concrete requirement/design/task counts, and say whether SVG previews were actually rendered. A generation turn finishing is not the same as the project plan being approved or its execution tasks being completed.

The Skills define the happy path; the completion proof is only a guard against accidental false success. Do not aim merely to satisfy validation. Aim to leave the user with a readable, substantive plan whose promised content is already visible inside Solution Studio.

Read [references/workspace-schema.md](references/workspace-schema.md) before writing a complete workspace document.

## Invariants

- One ChatOS project has exactly one project profile, one total requirements document, one overall design, and one execution plan. Different requests or `artifactKey` values update that plan; they do not define parallel workspaces.
- `artifactKey` and `sourceMode` are optional compatibility inputs. Prefer the host context and omit them unless maintaining an older caller.
- Treat `scope.projectId`, `scope.projectName`, `scope.connectorWorkspaceId`, and `scope.projectRoot` as host-supplied context. Never ask the user to enter them, invent them, or add them to upsert arguments. The service binds newly created workspaces to the active ChatOS project.
- Do not confuse the Solution Studio `workspaceId` with `scope.connectorWorkspaceId`: the former identifies the project's single planning document; the latter identifies the ChatOS connector workspace carrying the project root.
- Use stable IDs: `E-*` for evidence, `R-*` for requirements, `D-000-B-*` for project-level design blocks, `D-*` for requirement designs, `D-*-B-*` for typed design blocks, `ADR-*` for decisions, and `T-*` for tasks.
- The requirements document contains the project background and total requirements before its structured child requirements.
- The design document contains one project-level technical baseline and overall architecture. Each requirement then owns exactly one detailed design section and one execution-plan slice. Alternatives belong in design decisions, not parallel solution sections.
- Treat `tasks[].dependsOn` as the only source of execution order. Never encode order only in prose or canvas positions.
- Do not claim that plan generation authorizes implementation. A plan records work; it does not perform it.
- Keep the plugin self-contained: design SVG and the execution DAG are rendered only from this workspace's own data, with no cross-plugin references or dependencies.
- While this Skill is active, treat the host project directory as a read-only evidence source. Do not create planning `.md`, `.svg`, `.json`, screenshots, or other Solution Studio deliverables under `scope.projectRoot`. Store planning text and SVG code only through Solution Studio tools. Temporary visual-QA files may exist only outside the project directory and are not deliverables.
- A successful file write, terminal command, or model response is not Solution Studio delivery evidence. Only saved workspace content plus a matching `solution_finalize` completion proof closes the planning turn.
