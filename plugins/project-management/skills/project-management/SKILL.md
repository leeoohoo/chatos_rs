---
name: project-management
description: Manage local requirements, technical documents, work items, dependencies and versioned plans in the client-bound ChatOS project.
---

# Project planning

This plugin owns planning business data. It does not own the host project, Git repository or Task Runner execution state.

1. Read `planning_read` before editing. Use the current scope revision; never invent project IDs, directories, owners or execution credentials.
2. Use `planning_get` for full requirements, technical documents and work items. Keep acceptance criteria explicit and verifiable.
3. Apply `planning_change` with the current `expectedRevision` and a fresh stable `requestId`. Retry the exact same request ID/content after an uncertain response. On revision conflict, reread and reconcile; do not overwrite a user's edits.
4. Create requirements as drafts. Save non-empty technical documents before approval/readiness. Work items belong to one requirement. Dependency graphs and parent hierarchies must be acyclic.
5. A plan is a frozen version containing approved requirements, technical documents, ready work items and the full selected prerequisite scope. Revised business data requires a new plan version.
6. Business acceptance is separate from runtime success. Never mark a requirement accepted merely because a task succeeded.
7. Plan approval does not start execution. This version does not yet have the host execution bridge: explicitly report that limitation when asked to execute. Do not call the old project microservice, invent task/run IDs or write execution statuses into plugin data.

There is no create/list/rename/delete project tool, server database, Git tool, compatibility provider or device-scope fallback. A missing bound project must fail closed.
