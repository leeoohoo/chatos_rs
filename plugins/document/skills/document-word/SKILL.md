---
name: document-word
description: Create, edit, inspect, validate, and visually verify DOCX artifacts with structured paragraphs, headings, lists, tables, and bounded formatting operations.
metadata:
  chatos.role: leaf
---

# Word documents

Inspect an existing DOCX before editing. Identify its page count, sections, paragraphs, tables, and current structure. Extract text when exact wording matters and render pages when layout matters.

Use `office_create` with `format: "docx"` for a new document and `office_edit_batch` for a copied revision. Prefer semantic heading levels, concise paragraphs, real list operations, and structured tables over text that visually imitates those elements.

For a deliberate chapter or section boundary, set `pageBreakBefore: true` on the first `word_add_heading` or `word_add_paragraph` of the new page. Do not simulate pagination with repeated blank paragraphs.

Keep each batch coherent and ordered. Preserve content outside the requested scope. After editing, validate the output artifact and render the title page plus every page whose layout was materially changed.

Read [Word examples](references/examples.md) before a multi-section deliverable.
