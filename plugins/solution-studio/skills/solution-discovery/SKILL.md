---
name: solution-discovery
description: Analyze an existing project or greenfield brief and create evidence-backed, testable requirements. Use when goals, scope, constraints, users, acceptance criteria, or open questions need to be established before solution design.
---

# Solution Discovery

Produce a `RequirementsDocument` with a clear boundary between facts, user statements, assumptions, and unanswered questions.

For an existing project, inspect the smallest relevant set of source files, configuration, tests, and documentation. Record concrete paths or identifiers in `evidence`; do not infer current behavior from filenames alone. For a greenfield project, use the user's brief as `user-stated` evidence and mark unconfirmed choices as assumptions or open questions.

Requirements must describe observable outcomes rather than implementation steps. Give every requirement a stable ID, priority, evidence links, and acceptance criteria that a reviewer can verify. Use only the user-facing priority names `高`, `中`, and `低`; encode them as `must`, `should`, and `could` respectively in the workspace schema. Explicitly record in-scope and out-of-scope items when the boundary affects estimates or architecture.

Keep the project itself separate from its requirements. The active ChatOS project has exactly one project profile; send its background, overall description, project type, delivery form, and target platforms as `projectProfile` in `solution_upsert_requirements`, together with the workspace `description`, instead of creating another profile or workspace. These fields are required delivery content, not optional prose. Use `parentRequirementId` when a requirement is a decomposed part of another requirement; leave it absent for top-level requirements. Parent requirements state the broader outcome, while children add independently verifiable scope without duplicating the parent.

Read [the workspace schema](../solution-studio/references/workspace-schema.md), then call `solution_upsert_requirements` with the description, complete project profile, and complete requirements document. Preserve existing valid content when revising; do not silently remove prior requirements. For a requirements-only request, call `solution_finalize` with `scope: "requirements"` and an empty `expectedDesignBlocks` list after the upsert.
