---
name: chatos-async-task-orchestration
description: Plan and manage durable local work with the bound Task tools, including one Task or a multi-Task dependency graph, capabilities, follow-ups and handoff. Use for explicit requests to create background Tasks, durable requirements/solution/implementation plans, real project inspection, file creation/editing, commands, plugins or tracked execution. Ordinary questions, screenshot interpretation, translation and short reply drafting default to Main Chat unless the user explicitly requests a Task.
---

# Asynchronous Task orchestration

Use Task tools as an execution system, not a reminder list. Answer directly when the current context is sufficient. Create durable work when the user explicitly requests a background Task or when the request needs real files, commands, builds, external capabilities, generated artifacts, or tracked execution. Preserve the requested scope even when the content is simple, such as drafting but not sending an email.

Structured requirements, solution design, and dependency-aware implementation plans are durable artifacts when the current catalog advertises a matching planning Plugin. Bind that Plugin to the Task. Do not answer only in Main Chat or substitute a project Markdown file unless the user explicitly requested Markdown or no matching planning Plugin is available. A confirmed requirement plan is Task context, not a substitute for the requested planning artifact.

When the program-owned `user_selected_capability_contract` says the user chose a Plugin in the composer, that Plugin is the required delivery route. Create the requested durable Task with that exact selection; do not ask for its version or internal key, silently substitute another Plugin, answer only in Main Chat, or use Notepad as a fallback. Do not insert a Requirement Survey before the selected Plugin Task: choosing a Plugin and choosing Requirement Survey are distinct composer actions. If the selected Plugin needs a genuinely blocking task-local detail, let that Task request it through Ask User. The Host freezes and enforces the selected Release independently of these instructions.

An attachment alone does not require a Task. Main Chat can read an image with `local_attachment_read` using `delivery=model`, or text with `delivery=content`, and answer ordinary interpretation, translation, or drafting requests directly. A checksum or returned path is not proof that the content was seen. For an actual Task, choose `delivery=model` for model input or `delivery=workspace` when the file must also enter the Task workspace.

## Choose the work shape

Before dispatch, confirm the real goal, audience, material constraints, priorities, and acceptance expectations. If consequential information is missing and the user did not explicitly select a Plugin, activate `chatos-requirement-discovery`, create the Requirement Survey directly in Main Chat, and wait for submitted answers before dependent work. Do not create a survey-only placeholder Task or invent preferences. An explicit Plugin selection dispatches that Plugin Task directly; task-local blockers use Ask User. Clear requests proceed without a questionnaire.

- Use `create_task` for one coherent deliverable whose inputs are known and whose implementation and verification are tightly coupled.
- Use `create_tasks_with_prerequisites` for multiple independently verifiable deliverables, distinct specialties or capabilities, useful parallel work, real staged dependencies, or independent risk verification.
- Every graph needs one `graph_goal`, one `decomposition_rationale`, and at least two Tasks. Every Task needs a distinct owned outcome.

For compound work, read [complexity and decomposition](references/complexity-and-decomposition.md). Before creating a graph, read [graph patterns and dependencies](references/graph-patterns-and-dependencies.md). Use [acceptance examples and antipatterns](references/acceptance-examples-and-antipatterns.md) when the boundary between one Task and a graph is unclear.

## Define executable contracts

Every Task must state a concrete objective, `expected_outputs`, checkable `acceptance_criteria`, and only the inputs and capabilities it needs. Preserve the user's constraints exactly; do not strengthen, broaden, or invent exclusions. A `read-only` or `do not modify` constraint forbids mutation, but does not forbid non-mutating screenshots, snapshots, accessibility reads, listings, or file reads unless the user said so.

When program-bound `requirement_plan_context` exists, use only its current confirmed revision and the authority the user actually granted. A saved draft or submitted survey is not confirmation. Express Task-specific outputs and criteria rather than creating another plan snapshot. On a stale-plan rejection, return to the current plan instead of repeating the same call.

Read [capabilities and plugins](references/capabilities-and-plugins.md) before choosing built-ins, model configuration, or Plugin hints. Never invent a capability, model, reasoning level, Plugin key, or release absent from the current schema/catalog.

Do not select `Notepad` merely because notes might be useful, the worker needs scratch space, a plan should survive the Run, or information may be reusable. Select it only when the user explicitly asks to save something in Notepad or identifies an existing note as input. For an optional Notepad selection, set `notepad_user_request_quote` to the exact authorizing phrase from the current user message; never paraphrase or cite generic task text. If a Plugin artifact and a Notepad record are both requested, create a separate downstream Notepad Task; never give writable Notepad to the Plugin Task.

Give `CodeMaintainerWrite` Tasks narrow normalized project-relative `owned_paths`; an empty list claims the whole project. Declare known files written by Plugin Tasks. Before parallelizing, compare direct and indirect writes; if two Tasks may touch one source of truth, serialize them or give one Task write ownership. The exact lock, dependency, notification, and final-delivery rules are in [graph patterns and dependencies](references/graph-patterns-and-dependencies.md).

Set `final_response_max_chars` only for a user-requested bound on the complete textual response; it counts Unicode scalar values including Markdown and whitespace. Do not reinterpret file, body, or word limits as this field. A review Task succeeds by producing an evidence-based review, not by automatically approving the reviewed work.

## Load only the relevant domain reference

- Software implementation, debugging, migration, tests, or review: [software engineering](references/software-engineering.md)
- Product, visual, interaction, brand, or asset design: [design and creative](references/design-and-creative.md)
- Copywriting, documentation, editorial, marketing, or localization: [writing, content, and marketing](references/writing-content-and-marketing.md)
- Research, data analysis, comparison, audit, or operations: [research, data, and operations](references/research-data-and-operations.md)
- Documents, spreadsheets, presentations, PDFs, audio, image, or video deliverables: [documents, presentations, and media](references/documents-presentations-and-media.md)

Do not load every reference. Load only the reference needed to define the current Task or graph.

## Lifecycle

Treat history as context, not proof that the latest request is complete. If new intent changes active work, resolve the pending follow-up or cancel affected non-terminal Tasks before replacement, and use `supersedes_task_ids` where supported. Never call `cancel_task` for a `failed`, `succeeded`, or `cancelled` Task; it is already terminal, so create any wanted replacement directly and list the old ID in `supersedes_task_ids`. Read [follow-ups, cancellation, and recovery](references/followups-cancellation-and-recovery.md) before changing active work or retrying a failed branch.

After arranging all requested work, call `wait_for_task_completion` exactly once. It is a handoff signal, not polling. After it succeeds, call no more Task tools in that turn and tell the user concisely that work has started; do not imply completion or expose internal routing.
