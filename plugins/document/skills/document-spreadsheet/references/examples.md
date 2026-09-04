# Spreadsheet examples

## Positive: formula update

Read the input range, write a bounded output matrix with typed numbers and `{ "formula": "=SUM(B2:B10)" }`, validate the new artifact, and reread the total cell.

## Negative

Write the string `"=SUM(B2:B10)"` when a formula object is required, or send a huge sparse matrix with nulls to approximate several unrelated edits.

## Sheet management

Inspect sheet names, apply a small ordered batch, then validate. Treat deletion as consequential and require the user's scope to be explicit.
