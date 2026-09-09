# Validation examples

## Positive

Complete one bounded Hero typography Step, run draft validation, and capture the page. The screenshot still lacks a clear focal point, so retry that Step instead of advancing. In later invocations, complete the remaining visual Steps. Only after a whole-artboard screenshot passes the Design Gate should handoff validation run. Repair a required-viewport overflow as another Step, capture again, and export React only because the user requested code.

## Negative

Export immediately and report success because validation passed or buttons work, while the screenshot still looks generic, has structural errors, or contains an unresolved annotation.

## Blocking flattened mockup

If handoff reports `page_as_text_mockup`, split the text into real Header, navigation, content section, card, label, value, table cell, and button nodes. Do not silence the issue by shortening the text, renaming the layer, or changing its type while leaving the UI flattened.

## Tool-call recovery

If a node batch is rejected for size or parsing, reread the page and retry as smaller logical regions. A failed large call does not authorize an image/text fallback or replacement of unrelated pages.

## Focused task

A text-only edit may require rereading, one focused patch, and validation; it does not justify replacing the page or exporting code.
