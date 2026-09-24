# Organization and mutation scenarios

## Organizing notes

- Prefer a small stable folder hierarchy over creating a folder for every note.
- Use tags when one note belongs to several themes; use a folder when ownership or lifecycle is shared.
- Search by a distinctive title or phrase before creating a likely duplicate.

## Updating

Read the existing note when preserving prior sections, tags, or decisions matters. Keep facts from the tool result distinct from new interpretation. If the requested change is ambiguous, clarify whether to append, merge, or replace.

## Deleting and renaming

- Resolve the exact note or folder first.
- Use recursive folder deletion only when removal of every contained note is intended.
- Do not retry a timed-out or ambiguous mutation until a read/list confirms whether it already succeeded.

## Examples

Good: find the existing incident runbook, read it, update the recovery section, then verify the returned note.

Bad: create a second near-duplicate note without searching; delete an entire folder to remove one note; store an access token in a troubleshooting note.
