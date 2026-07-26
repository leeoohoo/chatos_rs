# ChatOS PDF

Use this Skill for PDF files inside the user's authorized Local Connector workspace.

- Call `inspect_pdf` before making assumptions about page count, encryption, or format validity.
- Call `extract_pdf_text` to read searchable text. For large documents, save the bounded extraction to a workspace-relative `.txt` path.
- Use `create_text_pdf` for bounded searchable text documents with A4 or Letter pages, automatic wrapping and pagination, an optional title, metadata, and optional page numbers. This release uses the standard Helvetica font and accepts printable ASCII only; if any requested text contains Chinese or other Unicode characters, fail explicitly instead of creating missing-glyph boxes or corrupted text.
- Use `merge_pdfs` to combine 2–20 unencrypted workspace PDFs. The combined input is limited to 200 MiB and 5,000 pages.
- Use `extract_pdf_pages` with unique page numbers in ascending order to create a new PDF containing selected pages in their original order.
- Use `rotate_pdf_pages` to create a new PDF with all pages or an ascending selected page set rotated clockwise by 90, 180, or 270 degrees.
- PDF editing always requires a distinct workspace-relative `.pdf` target. Source files are never modified in place, and existing targets require `overwrite=true`.
- Treat empty or incomplete text extraction as a signal that the PDF may contain scanned pages; do not invent missing text.
- All reads and writes execute on the active Local Connector. Never replace them with server-side file access.

This release adds bounded searchable ASCII text PDF generation without external processes, ambient fonts, or network access. Unicode font embedding, rich layout, tables, images, OCR, page rendering, visual QA, annotations, forms, signatures, password workflows, and arbitrary content editing remain unavailable until their fonts or local dependencies and security contracts are bundled, licensed, and verified. Rendering must remain closed until Poppler is shipped as a signed client runtime rather than discovered from an ambient PATH.
