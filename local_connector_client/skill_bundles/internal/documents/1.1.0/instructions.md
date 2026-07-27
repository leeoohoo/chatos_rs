# ChatOS Documents

Use this Skill to create, inspect, and safely edit DOCX files in the authorized local workspace.

- Use `inspect_docx` before editing to obtain bounded text and structure metadata, including paragraphs, headings, tables, page breaks, tracked changes, media, and comments presence.
- Use `create_docx` for a simple document composed of an optional title and ordered paragraphs.
- Use `create_structured_docx` for styled paragraph, table, and page-break blocks. Paragraph styles are limited to normal, title, subtitle, heading 1–3, and quote; alignment is limited to left, center, right, or justify.
- Use `append_docx_content` to append structured blocks before the final section properties while preserving the source archive's other verified ZIP entries.
- Use `replace_docx_text` only for exact matches contained inside one Word text run. It intentionally does not guess across multiple runs because doing so can destroy formatting boundaries.
- DOCX editing always requires a distinct workspace-relative `.docx` target. Source files are never modified in place, and existing targets require `overwrite=true`.
- Preserve user wording and requested order. Do not claim that comments, tracked changes, images, headers/footers, or arbitrary layout were edited unless a tool result explicitly confirms it.
- All operations execute on the active Local Connector; never claim a cloud path was written.

This release adds bounded structural creation and conservative editing without launching Microsoft Word or an external process. Embedded images, comments, tracked-change authoring, headers/footers, footnotes, arbitrary OOXML patching, PDF export, page rendering, and visual QA remain unavailable.
