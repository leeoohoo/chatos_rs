---
name: document
description: Route ChatOS Document MCP work across Word, spreadsheet, presentation, and PDF workflows while enforcing workspace-bounded reads, managed-artifact writes, inspection, and visual verification.
metadata:
  chatos.role: router
  chatos.related-skills: "document-word,document-spreadsheet,document-presentation,document-pdf"
---

# Document MCP

Use Document MCP for DOCX, XLSX, PPTX, and PDF files inside the bound ChatOS workspace or for managed artifacts returned earlier in the current session.

## Storage boundary

- Inputs use relative workspace paths or a returned managed-artifact `relativePath`.
- Never invent absolute paths, traverse parents, or follow untrusted links.
- Created and edited files are new managed artifacts. They are not written into the project tree and never overwrite the source file.
- Return the produced artifact metadata; do not claim a project file was replaced.

## Route by format

- Activate `document-word` for DOCX creation, editing, paragraphs, headings, lists, tables, and Word layout verification.
- Activate `document-spreadsheet` for XLSX ranges, formulas, sheets, tabular structure, and workbook verification.
- Activate `document-presentation` for PPTX slides, text boxes, ordering, backgrounds, and slide-by-slide visual review.
- Activate `document-pdf` for PDF inspection, forms, page extraction, merge, reorder, rotation, metadata, or Office-to-PDF conversion limitations.

Use this router activation as `parent_activation_ref`.

## Platform Skill protocol

Activate this router with `skill_skill_activate`, activate the relevant format leaf with this activation as `parent_activation_ref`, and pass the returned evidence in `skillEvidence`. `office_create` selects its required leaf from `format`; spreadsheet and PDF tools require their matching leaf. Evidence is injected only as a model argument and is validated and removed by ChatOS before local execution.

## MCP tool directory

- General inspection: `document_inspect` reads format and structure; `document_extract_text` extracts bounded semantic text; `document_render` renders selected pages or sheets; `document_validate` reopens and optionally renders an artifact.
- Conversion: `document_convert` creates a raster-fidelity PDF from DOCX, XLSX, or PPTX and must not be described as an editable or text-faithful conversion.
- Office creation/editing: `office_create` creates DOCX, XLSX, or PPTX from typed operations; `office_edit_batch` applies typed operations to a source Office file and produces a new artifact.
- Spreadsheet: `spreadsheet_read_range` reads a bounded range; `spreadsheet_write_range` creates a new workbook artifact with bounded cell changes; `spreadsheet_manage_sheets` adds, renames, removes, or reorders sheets in a new artifact.
- PDF composition: `pdf_merge` combines sources; `pdf_extract_pages` copies selected pages; `pdf_transform` reorders, duplicates, rotates, or updates metadata.
- PDF forms: `pdf_form_list` inspects fields; `pdf_form_fill` fills declared fields into a new PDF artifact.

## Common workflow

Inspect before editing. For content questions, extract bounded text. For layout-sensitive work, render representative pages. After creation or editing, run `document_validate` and render pages that can expose layout defects before reporting completion.

Do not use one massive batch when separate artifacts or independently verifiable steps are clearer. Keep operations bounded and preserve source files.
