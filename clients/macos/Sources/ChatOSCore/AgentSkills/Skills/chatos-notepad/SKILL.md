---
name: chatos-notepad
description: Persist, organize, find, read, or update durable ChatOS notes. Use only when the user explicitly asks to record or save something in Notepad, or identifies an existing note as an input; do not use for scratch work, internal plans, ordinary answers, or another Plugin's deliverable.
---

# Durable notes

Use Notepad only when the current user request or program-owned Task contract explicitly requires a Notepad record, or identifies an existing note as an input. The fact that information is reusable, may help later, or should survive the Run is not sufficient authorization to create or update a note.

When explicitly requested, suitable note content can include design decisions, runbooks, research summaries, debugging records, prompt versions, or other reusable material. Do not use Notepad as model scratch space, a hidden execution log, generic plan storage, cross-Task memory, or a substitute for a selected Plugin workspace or managed artifact.

- Initialize storage on first use or after an explicit initialization error.
- Search or list before creating when a matching note may already exist.
- Use folders for durable subject boundaries and tags for cross-cutting retrieval.
- Create a note for a new durable artifact; update the exact existing note when revising it.
- Read the current note before an update whose correctness depends on existing content.

Do not save secrets, transient tool output, hidden reasoning, or a normal response the user did not ask to preserve. If a Task has a selected Plugin, read an explicitly named source note when needed, but do not create or update a note from that Plugin Task; a separately scoped downstream Notepad Task owns any explicitly requested note deliverable. Treat rename, delete, recursive folder deletion, and replacement of substantial content as mutations requiring an exact target.

Read [references/organization-and-mutations.md](references/organization-and-mutations.md) for folder/tag choices, deduplication, safe updates, deletions, and recovery.
