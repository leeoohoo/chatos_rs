---
name: document-word
description: Create, edit, inspect, validate, and visually verify DOCX artifacts with structured paragraphs, headings, lists, tables, and bounded formatting operations.
metadata:
  chatos.role: leaf
---

# Word documents

Inspect an existing DOCX before editing. Identify its sections, paragraphs, tables, and current structure. DOCX page count depends on rendering and is not provided by structural inspection; do not guess it. Extract text when exact wording matters and render pages when layout matters.

Use `office_create` with `format: "docx"` for a new document and `office_edit_batch` for a copied revision. Prefer semantic heading levels, concise paragraphs, real list operations, and structured tables over text that visually imitates those elements.

For a new polished document, set the page layout deliberately and define reusable paragraph styles before adding content. Use `word_set_document_layout` for page size and margins, `word_define_style` for the document's body/title/section styles, and the paragraph formatting fields for font, size, color, spacing, indentation, `keepNext`, `keepLines`, and widow control. Use `word_format_text` only for a bounded phrase that needs different run formatting. Use `word_add_header_footer` for page numbers or restrained repeating information.

`word_add_list_item.text` is plain content. Never include a bullet, dash, or number prefix in it; Word creates the visible list marker. Keep headings with their following content and avoid forced page breaks unless the user explicitly wants a new page.

For a deliberate chapter or section boundary, set `pageBreakBefore: true` on the first `word_add_heading` or `word_add_paragraph` of the new page. Do not simulate pagination with repeated blank paragraphs.

Keep each batch coherent and ordered. Preserve content outside the requested scope. After editing, validate the output artifact and call `document_render` for the title page plus every page whose layout was materially changed. Inspect the returned inline images; do not claim the visual result is good based only on file validation or PNG dimensions. Fix visible defects and render again before delivery.

Read [Word examples](references/examples.md) before a multi-section deliverable.

Start previewing page 1 and continue with at most two pages per call. Check `skippedPages` and `renderStatus`: a uniform DOCX canvas is reported as `REQUESTED_PAGE_EMPTY_OR_UNAVAILABLE` and is omitted from the normal page artifacts and inline images. It may indicate an out-of-range page or a rendering problem; do not infer a precise page count or treat it as visual approval. Intentional blank paper pages with a visible page boundary are retained.
