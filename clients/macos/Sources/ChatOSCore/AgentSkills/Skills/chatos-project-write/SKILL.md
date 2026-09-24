---
name: chatos-project-write
description: Modify files inside the bound ChatOS project through an atomic edit session. Use for creating, replacing, appending, or deleting files; always stage changes, review the staged result, then explicitly commit or abort.
---

# Transactional project file editing

All mutations belong to one run-scoped edit session:

1. Read the current targets and decide the smallest coherent change.
2. Open or reuse an edit session. Use a fresh session only when intentionally abandoning reuse semantics.
3. Stage an ordered batch. Supply expected hashes or exact context when available so stale content fails rather than being overwritten.
4. Inspect the staged result and changed paths. Correct the batch before commit if it does not match the intended outcome.
5. Commit once the entire batch is valid, or abort when the plan changes, validation fails, or the task is cancelled.

`stage_edit_batch` does not write project files, and a successful stage is not completion. `commit_edit_session` may reject concurrent changes; re-read and deliberately restage instead of weakening the precondition. Keep related edits atomic, but do not mix unrelated cleanup into the transaction.

After commit, verify the observable result with project reads and proportionate tests. Never claim a committed edit compiled, rendered, migrated, or deployed unless that separate verification occurred.

Read [references/transactions-and-conflicts.md](references/transactions-and-conflicts.md) for operation selection, multi-file batches, conflicts, deletions, cancellation, and positive/negative examples.
