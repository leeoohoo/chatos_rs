# Team asset lifecycle

## Durable or transient

Good asset: approved architecture, current setup guide, durable convention, recorded decision, or maintained project reference.

Not an asset: a routine status ping, raw tool log, personal scratch note, unconfirmed proposal, or duplicate Todo progress.

## Create versus update

List current assets and read the closest match. Update the existing subject rather than creating dated duplicates.

## Revision conflict

When an update is stale, re-read the asset, merge the other change intentionally, and retry with the new revision. Do not remove concurrency checks.

## Executor suggestions

Use `team_asset_review_list` during every manager cycle. For an applicable suggestion, list and read the current asset, create or update it, read the stored revision back, and resolve the review as `applied` with that `asset_ref`. Resolve as `rejected` only when the candidate is transient, duplicated, unsupported, secret, or otherwise unsuitable, and record the concrete reason.

Do not resolve first and promise to write later. Do not treat chat acknowledgement, a dashboard update, or an unrelated project document as shared-asset maintenance.

## Archive

Archive only the exact current asset reference after confirming it is obsolete or superseded. Archiving is not deletion of project history and should not be used to hide a disagreement.
