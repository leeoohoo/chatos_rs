---
name: document-pdf
description: Inspect, render, validate, merge, extract, reorder, rotate, edit metadata, and fill PDF forms while preserving page order and reporting conversion fidelity accurately.
metadata:
  chatos.role: leaf
---

# PDF workflows

Inspect the PDF before mutation. Render selected pages when visual content, form placement, rotation, or page order matters.

- `pdf_merge` combines 2–20 inputs in the given order.
- `pdf_extract_pages` copies selected 1-based pages into a new artifact.
- `pdf_transform` reorders or duplicates pages, rotates them, and updates metadata.
- `pdf_form_list` discovers AcroForm field names and types; use it before `pdf_form_fill`.
- `document_convert` converts DOCX/XLSX/PPTX to an image-based PDF preview.

Never guess form field names or page numbers. Preserve the source PDF and validate the output. For form filling, reread fields or render affected pages before claiming completion.

Office-to-PDF conversion is raster, non-searchable, and preview fidelity; do not call it high-fidelity or searchable. Large workbooks, decks, or documents require explicit bounded page/sheet selection.

Read [PDF examples](references/examples.md) before merge, forms, or conversion.
