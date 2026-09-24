---
name: chatos-notepad
description: Persist, organize, find, read, update, or remove durable ChatOS notes. Use when the user asks to save reusable knowledge or when an existing note is the declared source of truth; do not persist every ordinary answer.
---

# Durable notes

Use Notepad for information intended to survive the current conversation: design decisions, runbooks, research summaries, debugging records, prompt versions, or other reusable material.

- Initialize storage on first use or after an explicit initialization error.
- Search or list before creating when a matching note may already exist.
- Use folders for durable subject boundaries and tags for cross-cutting retrieval.
- Create a note for a new durable artifact; update the exact existing note when revising it.
- Read the current note before an update whose correctness depends on existing content.

Do not save secrets, transient tool output, hidden reasoning, or a normal response the user did not ask to preserve. Treat rename, delete, recursive folder deletion, and replacement of substantial content as mutations requiring an exact target.

Read [references/organization-and-mutations.md](references/organization-and-mutations.md) for folder/tag choices, deduplication, safe updates, deletions, and recovery.
