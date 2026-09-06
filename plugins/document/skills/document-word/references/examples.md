# Word examples

## Positive

Inspect the source, copy it with `office_edit_batch`, update the requested heading and table cells, validate the new artifact, render the affected pages, then return the new artifact.

## Negative

Recreate the entire document from extracted plain text to change one paragraph. This discards styles, tables, headers, and unrelated user formatting.

## New report

Build a title, heading hierarchy, short paragraphs, lists, and tables with typed operations. Start a new chapter with `{ "type": "word_add_heading", "level": 1, "text": "Details", "pageBreakBefore": true }`. Validate structure and visually inspect representative pages for clipping and awkward page breaks.
