# ChatOS Document MCP

Workspace-bounded MCP tools for Word, Excel, PowerPoint, and PDF files.

The current implementation exposes 15 policy-annotated tools:

- `document_inspect`, `document_extract_text`, `document_render`, and `document_validate` for `.docx`, `.xlsx`, `.pptx`, and `.pdf`
- `document_convert` for offline DOCX/XLSX/PPTX to image-based PDF conversion
- `office_create` for safe DOCX/XLSX/PPTX artifact creation
- `office_edit_batch` for copying a workspace Office file, applying typed edits, and producing a new artifact
- `spreadsheet_read_range`, `spreadsheet_write_range`, and `spreadsheet_manage_sheets` for bounded typed cell and worksheet operations
- `pdf_merge`, `pdf_extract_pages`, `pdf_transform`, `pdf_form_list`, and `pdf_form_fill`

The Office operation whitelist includes Word page layout, reusable styles, paragraphs, headings, lists, tables, phrase formatting, headers/footers, and detailed paragraph typography; typed spreadsheet cells and sheet management; and PowerPoint slide/textbox creation, text updates, slide reordering/deletion, and bounded slide properties. Word list operations reject text that already contains a visible list marker to prevent duplicated bullets or numbers.

PDF pages are rendered by the bundled PDFium WebAssembly module. Office pages are rendered one at a time by the pinned OfficeCLI HTML renderer. A render call creates individual PNG artifacts plus a JSON manifest with dimensions, hashes, engine information, and warnings. It also returns the first two requested pages as standard MCP image content so the model can directly inspect the current visual result; inline previews are resized to stay within the ChatOS transient-image budget and base64 data is never persisted in structured results.

The implementation enforces a 100 MiB input limit, at most 50 rendered pages per call, bounded DPI/viewports and a 100-million-pixel total render limit.

DOCX requests can produce a uniform viewer canvas when a page is unavailable. `document_render` reports these entries in `skippedPages` with `REQUESTED_PAGE_EMPTY_OR_UNAVAILABLE` and `renderStatus: "needs_review"`, without publishing them as normal pages or inferring page count. `document_validate` also reports the requested render check as requiring review instead of passing it. This conservative check retains sparse pages and intentional blank paper pages with visible boundaries; it does not determine Word's native pagination.

`document_convert` renders Office content with the pinned OfficeCLI HTML preview engine and assembles the PNG pages into a PDF with `pdf-lib`. The result is explicitly reported as `conversionMode: "raster"`, `searchableText: false`, and `layoutFidelity: "preview"`. DOCX converts up to the first 50 pages, PPTX supports selected slide order, and XLSX creates one page per selected worksheet (including an explicit blank page for an empty sheet). Presentations or workbooks with more than 50 slides/sheets require an explicit selection of at most 50 items.

High-fidelity, searchable Office-to-PDF export is not yet available: the pinned OfficeCLI release delegates native PDF export to an exporter plugin that is not included in its release binaries. Adding that mode still requires a separately pinned, licensed, offline exporter; the MCP will not download one at runtime.

## Development

```bash
npm ci --ignore-scripts
npm run sbom:generate
npm test
npm run vendor:verify:all
npm run pack:verify
```

`npm run sbom:generate` derives `SBOM.cdx.json`, `THIRD_PARTY_LICENSES.txt`, and `PDFIUM_THIRD_PARTY_NOTICES.txt` from the actual esbuild production input graph and pinned vendor manifests. The fixed PDFium `chromium/7243` inventory covers 16 linked/runtime components, their source revisions, binary/build evidence, required attribution, original license texts, and SHA-256 hashes. Normal tests fail if any generated compliance file becomes stale. `pack:verify` checks every notice/license asset and the package limits, safely extracts the candidate `.tgz`, starts the MCP directly from the extracted package without installing dependencies, verifies all 15 tools, and executes the bundled OfficeCLI to create a smoke-test DOCX.

Run locally:

```bash
CHATOS_WORKSPACE=/absolute/path/to/workspace \
CHATOS_PLUGIN_ARTIFACT_DIR=/absolute/path/to/artifacts \
CHATOS_PLUGIN_ROOT="$PWD" \
node bin/chatos-document-mcp mcp
```

Only relative paths are accepted. Input tools resolve the bound `CHATOS_WORKSPACE` first, then allow a `relativePath` returned for a managed artifact created earlier in the same MCP session. Symbolic links and path traversal are rejected. New or modified files are written only to `CHATOS_PLUGIN_ARTIFACT_DIR`, published through MCP `_meta["chatos/artifacts"]` for Host registration, and never overwrite an existing artifact. They are downloadable task artifacts rather than files written directly into the project workspace.

This project is licensed under Apache-2.0 and is configured for public npm publication. The final npm package scope still depends on which npm user or organization will publish it.
