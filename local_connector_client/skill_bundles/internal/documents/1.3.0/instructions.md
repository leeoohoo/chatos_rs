# ChatOS Documents

Use this Skill to create, inspect, and safely edit DOCX files in the authorized local workspace.

- Use `inspect_docx` before editing to obtain bounded text and structure metadata, including paragraphs, headings, tables, page breaks, tracked changes, media, comment count and text preview, headers, footers, and bounded header/footer text previews.
- Use `create_docx` for a simple document composed of an optional title and ordered paragraphs.
- Use `create_structured_docx` for styled paragraph, table, and page-break blocks. Paragraph styles are limited to normal, title, subtitle, heading 1–3, and quote; alignment is limited to left, center, right, or justify.
- Use `append_docx_content` to append structured blocks before the final section properties while preserving the source archive's other verified ZIP entries.
- Use `replace_docx_text` only for exact matches contained inside one Word text run. It intentionally does not guess across multiple runs because doing so can destroy formatting boundaries.
- Use `insert_docx_image` to append one workspace PNG or JPEG. The image must be at most 10 MiB, have a valid supported signature and bounded dimensions, and stay within 40 megapixels. The tool preserves aspect ratio, fits the image inside a bounded page area, and supports accessible alt text.
- Use `add_docx_header_footer` only when the document does not already contain the corresponding header or footer reference. It adds default text parts to the final section and intentionally refuses to replace or merge complex existing section headers and footers.
- Use `add_docx_comment` only when the selected wording is the complete text of one eligible Word text run. It can create the standard comments part or append to an existing standard comments part, but it will not guess across runs, comment a substring, nest inside an active comment range, or attach to drawings, fields, tabs, or breaks.
- DOCX editing always requires a distinct workspace-relative `.docx` target. Source files are never modified in place, and existing targets require `overwrite=true`.
- Preserve user wording and requested order. Do not claim that tracked changes, existing complex headers/footers, arbitrary image placement, or arbitrary layout were edited unless a tool result explicitly confirms it.
- All operations execute on the active Local Connector; never claim a cloud path was written.

This release adds conservative exact-run comment authoring without launching Microsoft Word or an external process. Tracked-change authoring, replacement of existing multi-section headers/footers, floating or wrapped images, footnotes, arbitrary OOXML patching, PDF export, page rendering, and visual QA remain unavailable. Rendering tools must stay closed until Poppler or LibreOffice is shipped as a verified client runtime instead of being discovered from an ambient PATH.
