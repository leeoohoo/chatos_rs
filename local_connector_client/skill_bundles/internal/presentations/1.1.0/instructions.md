# ChatOS Presentations

Use this Skill to inspect or create bounded editable PPTX presentations inside the authorized local workspace.

- Use `inspect_pptx` before delivery. It validates package paths and expanded size, reports the exact widescreen slide dimensions, ordered slide files, per-slide title/text previews, internal image relationships, media-file count, and speaker-note previews.
- Use `create_pptx` for 1–200 ordered widescreen slides. Supported layouts are `title_body`, `title_only`, `section`, `two_column`, `image_right`, and `image_full`.
- `title_body` is the default compatibility layout. `section` creates a centered section divider. `two_column` uses `left_body` and `right_body`. `image_right` and `image_full` require an `image` object and reject missing images rather than silently changing layout.
- Lines beginning with `- ` or `* ` become editable DrawingML bullet paragraphs. Other lines remain ordinary editable text paragraphs.
- Images must be workspace-local PNG or JPEG files, at most 10 MiB each, at most 20000 pixels per edge and 40 megapixels. Combined image input is limited to 50 MiB. `contain` preserves the full image inside its box; `cover` uses bounded centered DrawingML cropping. Alt text is required and is written to the picture description. Source image files are read-only and never modified.
- Optional `notes` become standard editable notesMaster/notesSlide speaker notes rather than visible slide text. Notes and slides remain local package parts; no cloud presentation service is contacted.
- Deck text is limited to 500000 characters total, 100000 characters and 2000 lines per slide field. XML-incompatible control characters fail before any output is persisted.
- PPTX output uses a same-directory temporary file, defaults to refusing existing targets, rejects symlink targets, duplicate/unsafe ZIP entries, more than 10000 entries, and compressed or expanded output above 100 MiB.
- The writer uses a stable built-in 16:9 theme and editable text/image shapes. It does not currently edit an existing deck, create charts/tables/SmartArt, import arbitrary themes/layouts, add transitions or animations, render slide images/PDF, or perform visual QA. Do not imply those operations were applied.
- Never claim that PowerPoint, Keynote, LibreOffice, Google Slides, or another desktop/cloud presentation application was controlled. The artifact is generated locally by the active Local Connector.
