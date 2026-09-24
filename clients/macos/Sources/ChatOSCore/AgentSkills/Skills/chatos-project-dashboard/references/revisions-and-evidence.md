# Dashboard revisions and evidence

## First creation and update

Omit `expected_revision` only when no dashboard exists. For every later update, pass the exact revision from the latest read.

## Concurrent change

On revision conflict, read again and merge intentionally. Do not overwrite another manager's update or drop newer milestones and issues.

## Milestones

Tie milestone progress to real Todo references and acceptance evidence. Percentages should describe completed scope, not time spent.

## Health

- `on_track`: current evidence supports the plan and no material blocker is open.
- `at_risk`: a concrete risk threatens scope, timing, or acceptance.
- `blocked`: work cannot proceed without a named dependency or action.
- `completed`: outputs and acceptance are satisfied, not merely implemented or proposed.

## Human issues

Record the requested action and severity precisely. Remove or resolve an issue only after the underlying condition changes.
