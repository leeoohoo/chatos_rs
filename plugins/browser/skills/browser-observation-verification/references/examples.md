# Verification examples

## Positive

After saving settings, take a fresh snapshot and verify the displayed saved value or success state. If the page redirects, also verify the destination URL and page identity.

## Negative

Report “saved” because `browser_click` returned normally. The button may have been disabled, validation may have failed, or the request may still be pending.

## Visual-only content

For a chart or canvas, use a screenshot and describe only visible labels and trends. Do not pretend the accessibility tree contains underlying data that was not returned.
