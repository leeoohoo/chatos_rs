# Interaction examples

## Positive: dynamic form

Snapshot the page, fill the visible required fields with current refs, verify entered values, open the dynamic selector, snapshot again, choose the new current option ref, and verify the final form state before submitting.

## Negative: stale refs

Use refs captured before opening a modal or changing tabs. Those refs may point to removed or different elements.

## Positive: bounded scrolling

Scroll one viewport-sized step, snapshot, and repeat only while content changes and the target remains below.

## Negative: blind action chain

Click, type, press Enter, and report success without any fresh observation. A transport-level success does not establish that focus, validation, or submission behaved as intended.
