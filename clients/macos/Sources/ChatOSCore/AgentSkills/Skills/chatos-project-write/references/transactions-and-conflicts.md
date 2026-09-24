# Edit transactions and conflicts

## Replace a unique block

Good: use `replace_text` with exact old text, appropriate surrounding context, and `expected_matches` when uniqueness matters. Stage and inspect before commit.

Bad: replace a short common token globally or omit match expectations when several occurrences would change semantics.

## Create or fully rewrite a file

Good: use `write` when the complete intended content is known. For an existing file, include the observed hash so a concurrent edit fails closed.

Bad: rewrite a large existing file just to change a few lines, silently discarding user formatting or nearby work.

## Append

Good: append only when ordering and duplication behavior are understood, and guard against repeating the same block on retry.

Bad: use append for structured configuration where placement, uniqueness, or syntax requires a targeted replacement.

## Delete

Good: confirm the exact target, references, and task authorization; use the observed hash when available and keep related reference updates in the same transaction.

Bad: delete a guessed path, generated-looking file, or broad directory because it seems unused.

## Multi-file invariant

Good: stage all files required for one coherent invariant in one ordered batch, inspect every changed path, then commit atomically.

Bad: commit half of a rename or schema change and plan to fix dependent files in a later transaction without a real reason.

## Concurrent modification

Good: when commit reports stale content, re-read the affected file, understand the other change, rebuild the minimal patch, and restage intentionally.

Bad: remove the expected hash or broaden replacement context merely to force the commit through.

## Cancellation or invalid plan

Good: abort the edit session and report that no staged changes were committed.

Bad: leave a staged session ambiguous, or say changes were reverted without confirming whether a commit had already occurred.

## Verification after commit

Good: read the changed region and run the smallest relevant test or validation when the task requires behavioral confidence.

Bad: equate “commit_edit_session returned success” with compilation, runtime correctness, deployment, or user-visible completion.
