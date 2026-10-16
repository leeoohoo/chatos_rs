# Word examples

## Positive

Inspect the source, copy it with `office_edit_batch`, update the requested heading and table cells, validate the new artifact, render the affected pages, then return the new artifact.

## Negative

Recreate the entire document from extracted plain text to change one paragraph. This discards styles, tables, headers, and unrelated user formatting.

## New report

Build a title, heading hierarchy, short paragraphs, lists, and tables with typed operations. Start a new chapter with `{ "type": "word_add_heading", "level": 1, "text": "Details", "pageBreakBefore": true }`. Validate structure and visually inspect representative pages for clipping and awkward page breaks.

For a polished Chinese document, start with an A4 layout and explicit margins, define reusable body and section styles with an appropriate CJK font, then add content. Keep section headings with the next paragraph. Pass `{ "type": "word_add_list_item", "text": "项目内容" }`, never `"• 项目内容"`.

`document_render` returns the first two requested pages directly to the model as images. Render at most two pages per review call, inspect them, correct visible layout problems, and render the corrected pages again.
