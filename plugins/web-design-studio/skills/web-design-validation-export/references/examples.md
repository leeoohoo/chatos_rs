# Validation examples

## Positive

Build a Header batch, run draft validation, build the Hero batch, run draft validation, finish the page, run handoff validation, fix a mobile overflow, validate again, then export React because the user requested code.

## Negative

Export immediately and report success while the design still has structural errors or an unresolved annotation.

## Blocking flattened mockup

If handoff reports `page_as_text_mockup`, split the text into real Header, navigation, content section, card, label, value, table cell, and button nodes. Do not silence the issue by shortening the text, renaming the layer, or changing its type while leaving the UI flattened.

## Tool-call recovery

If a node batch is rejected for size or parsing, reread the page and retry as smaller logical regions. A failed large call does not authorize an image/text fallback or replacement of unrelated pages.

## Focused task

A text-only edit may require rereading, one focused patch, and validation; it does not justify replacing the page or exporting code.
