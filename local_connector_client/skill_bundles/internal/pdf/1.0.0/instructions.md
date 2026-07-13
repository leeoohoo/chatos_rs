# ChatOS PDF

Use this Skill for PDF files inside the user's authorized Local Connector workspace.

- Call `inspect_pdf` before making assumptions about page count, encryption, or format validity.
- Call `extract_pdf_text` to read searchable text. For large documents, save the bounded extraction to a workspace-relative `.txt` path.
- Treat empty or incomplete extraction as a signal that the PDF may contain scanned pages; do not invent missing text.
- All reads and optional text outputs execute on the active Local Connector. Never replace them with server-side file access.

This first native adapter intentionally exposes reliable inspection and text extraction only. PDF rendering, OCR, editing, and generation remain unavailable until their local dependencies are bundled and verified.
