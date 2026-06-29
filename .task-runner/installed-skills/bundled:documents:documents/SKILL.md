---
name: documents
description: Create, edit, redline, and comment on `.docx`, Word, and Google Docs-targeted document artifacts inside the container, with a strict render-and-verify workflow. Use `render_docx.py` to generate page PNGs (and optional PDF) for visual QA, then iterate until layout is flawless before delivering the final document.
---

# Documents Skill (Read • Create • Edit • Redline • Comment)

Use this skill when you need to create or modify `.docx`, Word, or Google
Docs-targeted document artifacts **in this container environment** and verify
them visually.

## Tools + Contract

- Use Codex workspace dependencies for docx artifact work: resolve them through the workspace dependency loader or runtime skill, then treat the returned Node/Python runtimes and package directory as authoritative. Do not use system `node`, system `python`, global npm packages, or repo-local installs.
- For document creation and deterministic OOXML edits, it is still acceptable to use the bundled Python/OOXML helper scripts in this skill package when the JS surface is incomplete.
- Run any builder or helper file from a writable workspace or temp directory, not from the managed dependency directory itself.
- Final user-facing responses should describe only the requested document result
  and link only to the final `.docx`, Word, or Google Docs deliverable unless
  the user explicitly asks for QA intermediates.

## Google Docs-targeted output

For a net-new Google Docs request, create and visually verify a local `.docx` with this skill first. The native Google Docs deliverable must then be produced by the Google Drive plugin's document import action, `mcp__codex_apps__google_drive_import_document`, with `upload_mode: "native_google_docs"`.

Before rendering or importing any Google Docs-targeted DOCX, run the deterministic title sanitizer:

```bash
python scripts/google_docs_title_sanitize.py input.docx --out sanitized.docx
python scripts/google_docs_title_sanitize.py sanitized.docx --check
```

Use the sanitized DOCX for render QA and native Google Docs import. This is not a style preference or prose reminder: the sanitizer removes Word `Title` paragraph-style border residue, direct title-paragraph borders, and leading title-block paragraph borders from the OOXML so Word's built-in blue title rule cannot survive into the imported Google Doc.

Do not use Computer Use, Browser Use, blank-Google-Doc creation plus Google Docs write APIs, or another direct-to-Docs construction path for net-new Google Docs unless the user explicitly asks for that alternate workflow. If they do, mention first that output quality is expected to be best when a local `.docx` is imported through the Google Drive plugin.

If the Google Drive plugin is unavailable, use the plugin-install/user-elicitation flow to ask the user to install `google-drive@openai-curated`. If the plugin is available but `_import_document` is missing, ask the user to reinstall or refresh the Google Drive plugin before continuing with the native Google Docs deliverable.

## Non-negotiable: render → inspect PNGs → iterate

**You do not “know” a DOCX is satisfactory until you’ve rendered it and visually inspected page images.**
DOCX text extraction (or reading XML) will miss layout defects: clipping, overlap, missing glyphs, broken tables, spacing drift, and header/footer issues.

**Shipping gate:** before delivering any DOCX, you must:
- Run `render_docx.py` to produce `page-<N>.png` images (optionally also a PDF with `--emit_pdf`)
- Open the PNGs (100% zoom) and confirm every page is clean
- If anything looks off, fix the DOCX and **re-render** (repeat until flawless)

If rendering fails because LibreOffice/`soffice` is missing, it is acceptable to return the requested DOCX without rendered PNG QA. In that fallback case, use the relevant Markdown task docs in this skill package as the authoritative guidance for building and checking the document structurally, state clearly in the final response that rendering/visual QA could not be completed, and do not imply that the document passed the render gate.

If rendering fails for any other reason, fix rendering first (LibreOffice profile/HOME, conversion errors, or renderer setup) rather than guessing.

**Deliverable discipline:** Rendered artifacts (PNGs and optional PDFs) are for internal QA only. Unless the user explicitly asks for intermediates, **return only the requested final deliverable** (e.g., when the task asks for a DOCX, deliver the DOCX — not page images or PDFs).




## Design Preset Contract

For new DOCX creation and major rewrites, a design preset is mandatory unless the user explicitly asks for a different visual system. For existing-document edit tasks, preserve the original document and apply the minimal local edits described later in this skill.

Picking a preset is not enough. You must resolve the preset into exact numeric tokens and apply those numbers in the DOCX implementation. Do not rely on Word defaults, built-in list styles, theme defaults, inherited paragraph spacing, or renderer-dependent behavior for any preset-controlled value.

Before writing content, read `references/design_presets.md` and choose exactly one preset:

- `google_docs_default` for any net-new document whose destination is a native Google Doc, unless the user explicitly asks for a special, branded, or highly polished visual treatment.
- `standard_business_brief` for formal memos, RFI responses, decision memos, and board-style briefs.
- `compact_reference_guide` for launch guides, negotiation briefs, checklists, and dense operator references.
- `narrative_proposal` for grants, proposals, and persuasive documents with longer prose.
- Use an archetype alias from the reference file when it is a closer match: `rfi_response`, `decision_memo`, `launch_messaging_guide`, `contract_negotiation_brief`, `neighborhood_business_proposal`, or `grant_proposal`.

If the destination is Google Docs, choose `google_docs_default`. Google Docs-targeted documents should feel native: Arial-based typography, black hierarchy, simple title treatment.

For Google Docs-targeted documents, never create the title with the built-in Word `Title` paragraph style, including `doc.add_paragraph(..., style="Title")` or `doc.add_paragraph(style="Title")`. Always create a plain paragraph and apply the selected style-sheet title tokens directly: font family, size, color, weight, spacing, and border/rule settings. For `google_docs_default`, that means Arial 26 pt, black, normal weight, 0 pt before, 3 pt after, and no underline, bottom border, horizontal rule, or other Word-template residue. This instruction is not the enforcement layer; `scripts/google_docs_title_sanitize.py` is the deterministic enforcement layer and must still run before render/import.

If creating a new first-page header, cover, or title block for a non-Google-docs document, also read `references/header_templates.md` and choose one header pattern before drafting. For `google_docs_default`, keep the opening block simple unless the user explicitly requests richer first-page furniture.

Then resolve the preset into a token map and apply the tokens consistently:

1. Set page, margin, type scale, paragraph rhythm, heading, list, table, callout, header, footer, and color tokens before drafting. For `google_docs_default`, that means explicitly carrying the simple Google Docs defaults instead of inheriting the more polished Word-oriented defaults above.
2. Implement those tokens through Word styles, real numbering definitions, explicit table geometry, and header/footer parts. Do not fake headings, lists, or tables with one-off direct formatting.
3. Use ad-hoc formatting only when the document needs a specific exception; record the exception as a named override and reuse it consistently wherever that role appears.
4. Keep the preset stable throughout the document. Do not mix body spacing, heading colors, list indents, table fills, or page furniture from multiple presets.

Baseline geometry for all presets: US Letter portrait, 1 inch margins, 9360 DXA usable width, real Word styles for Normal/Title/Subtitle/Heading 1/Heading 2/Heading 3, real Word numbering for lists, and DXA table widths only.

Tables must use explicit Word geometry. Build rows first, compute exact DXA column widths, then use `scripts/table_geometry.py` or equivalent logic so `tblW`, `tblInd`, `tblGrid`, and every `tcW` agree. Set table indent to the start cell margin token (`120` DXA by default) so the visible outer border aligns with surrounding paragraph text. Do not rely on autofit, percentage widths, centered default tables, fixed row heights, or tables as layout/divider hacks.

Lists must use real numbering definitions. Never create fake bullets with Unicode bullet text, hyphen-prefixed paragraphs, manual numbers, or newline-separated list items inside one paragraph. Wrapped list lines must align under the item text, not under the marker.

Before final render review, run a preset audit: page geometry, styles, heading spacing/colors, list indents, table widths/table indents/cell margins, callout fills, headers/footers, and direct-formatting exceptions must match the selected token map. Also check for fake headings, fake bullets, missing table geometry, clipped/pinned table text, inconsistent page furniture, and unexplained direct formatting drift. For `google_docs_default`, fail the audit if the title style or title paragraph contains `w:pBdr`, a bottom border, an underline, a horizontal rule, or any rendered decorative line under the title.

## Form factor selection

For new DOCX creation and major rewrites, choose content form factors deliberately before drafting. Start from the information type, then calibrate the structure to the document archetype. Use the lightest readable structure that helps the reader understand, compare, act on, or fill in the information with the least friction.

First map each major content unit to a form factor:

- PROSE SECTION: narrative, explanation, background, or rationale. Use paragraphs under clear headings, with short supporting bullets only when they improve skimming.
- LEAD CALLOUT: decision, recommendation, or key takeaway. Use a short labeled paragraph, callout, or lead paragraph followed by concise rationale.
- NUMBERED STEPS: sequence, workflow, or procedure. Use step blocks with clear action verbs; add owner/status fields only when they are central to execution.
- GROUPED BULLETS: loose factors, considerations, pros/cons, or requirements. Use bullets or short subsections when order is not the main point.
- CHECKLIST: actions, acceptance checks, or review criteria. Use compact labels and enough spacing to scan.
- NOTE BOX: warnings, caveats, constraints, or important notes. Use a callout with restrained emphasis.
- DEFINITION LIST: definitions, metadata, or key facts. Use labeled paragraphs, definition lists, or compact key-value blocks.
- TABLE: repeated comparable records, status grids, budgets, RFI/compliance matrices, or schedules with shared fields.
- FORM LAYOUT: forms and questionnaires. Use readable fields, sectioning, and response space; use grids only where repeated response structure helps completion.
- SOURCE LIST: evidence, citations, and sources. Use footnotes, endnotes, short source lists, or appendices according to document type and density.

### Table Gate

Use a table only when the content is truly row/column data: repeated items, shared fields, and useful comparison or lookup.

Do not use tables to package normal prose. If cells become mini-paragraphs, switch to prose sections, bullets, steps, checklists, callouts, or appendix material.

Before finalizing, run a table-overuse audit:

- If most cells in a table are sentence- or paragraph-length prose, convert that section to prose, bullets, steps, callouts, or labeled paragraphs.
- If two or more adjacent sections use tables, check whether at least one should become bullets or paragraphs for readability.

During render review, check content diversity and archetype fit. If multiple adjacent components use the same visual form, decide whether one should become prose, bullets, steps, a callout, or an appendix. The goal is not variety for its own sake; it is to match form to reading task and document purpose.

## Design standards for document generation

For generating new documents or major rewrite/repackages, follow the design standards below unless the user explicitly requests otherwise. The user's instructions always take precedence; otherwise, adhere to these standards.

When creating the document design, do not compromise on the content and make factual/technical errors. Do not produce something that looks polished but not actually what the user requested.

It is very important that the document is professional and aesthetically pleasing. As such, you should follow this general workflow to make your final delivered document:

1. Before you make the DOCX, please first think about the high-level design of the DOCX:
   - Before creating the document, decide what kind of document it is (for example, a memo, report, SOP, workflow, form, proposal, or manual) and design accordingly. In general, you shall create documents which are professional, visually polished, and aesthetically pleasing. However, you should also calibrate the level of styling to the document's purpose: for formal, serious, or highly utilitarian documents, visual appeal should come mainly from strong typography, spacing, hierarchy, and overall polish rather than expressive styling. The goal is for the document's visual character to feel appropriate to its real-world use case, with readability and usability always taking priority.
   - You should make documents that feel visually natural. If a human looks at your document, they should find the design natural and smooth. This is very important; please think carefully about how to achieve this.
   - Think about how you would like the first page to be organized. How about subsequent pages? What about the placement of the title? What does the heading ladder look like? Should there be a clear hierarchy? etc
   - Which form factors should represent each type of information, such as prose sections, bullets, numbered steps, checklists, callouts, tables, forms, images, or appendices? Plan the design for each chosen component.
   - Think about the general spacing and layout. What will be the default body spacing? What page budget is allocated between packaging and substance? How will page breaks behave around tables and figures, since we must make sure to avoid large blank gaps, keep captions and their visuals together when possible, and keep content from becoming too wide by maintaining generous side margins so the page feels balanced and natural.
   - Think about font, type scale, consistent accent treatment, etc. Try to avoid forcing large chunks of small text into narrow areas. When space is tight, adjust font size, line breaks, alignment, or layout instead of cramming in more text.
2. Once you have a working DOCX, continue iterating until the entire document is polished and correct. After every change or edit, render the DOCX and review it carefully to evaluate the result. If LibreOffice/`soffice` is missing, continue using the relevant Markdown task docs in this skill package for structural QA and document-design guidance, and disclose that visual render QA was skipped. The plan from (1) should guide you, but it is only a flexible draft; you should update your decisions as needed throughout the revision process. Important: each time you render and reflect, you should check for both:

   1. Design aesthetics: the document should be aesthetically pleasing and easy to skim. Ask yourself: if a human were to look at my document, would they find it aesthetically nice? It should feel natural, smooth, and visually cohesive.
   2. Formatting issues that need to be fixed: e.g. text overlap, overflow, cramped spacing between adjacent elements, awkward spacing in tables/charts, awkward page breaks, etc. This is super important. Do not stop revising until all formatting issues are fixed.

While making and revising the DOCX, please adhere to and check against these quality reminders, to ensure the deliverable is visually high quality:

- Document density: Try to avoid having verbose dense walls of text, unless it's necessary. Avoid long runs of consecutive plain paragraphs or too many words before visual anchors. For some tasks this may be necessary (i.e. verbose legal documents); in those cases ignore this suggestion.
- Font: Use professional, easy-to-read font choices with appropriate size that is not too small. Usage of bold, underlines, and italics should be professional.
- Color: Use color intentionally for titles, headings, subheadings, and selective emphasis so important information stands out in a visually appealing way. The palette and intensity should fit the document's purpose, with more restrained use where a formal or serious tone is needed.
- Visuals: Consider using varied form factors, including diagrams and other visual components, when they improve comprehension, navigation, or usability.
- Tables: Please invest significant effort to make sure your tables are well-made and aesthetically/visually good. Below are some suggestions, as well as some hard constraints that you must relentlessly check to make sure your table satisfies them.
  - Suggestions:
    - Set deliberate table/cell widths and heights instead of defaulting to full page width.
    - Choose column widths intentionally rather than giving every column equal width by default. Very short fields (for example: item number, checkbox, score, result, year, date, or status) should usually be kept compact, while wider columns should be reserved for longer content.
    - Avoid overly wide tables, and leave generous side margins so the layout feels natural.
    - Keep all text vertically centered and make deliberate horizontal alignment choices.
    - Ensure cell height avoids a crowded look. Leave clear vertical spacing between a table and its caption or following text.
  - Hard constraints:
    - To prevent clipping/overflow:
      - Never use fixed row heights that can truncate text; allow rows to expand with wrapped content.
      - Ensure cell padding and line spacing are sufficient so descenders/ascenders don't get clipped.
      - If content is tight, prefer (in order): wrap text -> adjust column widths -> reduce font slightly -> abbreviate headers/use two-line headers.
    - Padding / breathing room: Ensure text doesn't sit against cell borders or look "pinned" to the upper-left. Favor generous internal padding on all sides, and keep it consistent across the table.
    - Vertical alignment: In general, you should center your text vertically. Make sure that the content uses the available cell space naturally rather than clustering at the top.
    - Horizontal alignment: Do not default all body cells to top-left alignment. Choose horizontal alignment intentionally by column type: centered alignment often works best for short values, status fields, dates, numbers, and check indicators; left alignment is usually better for narrative or multi-line text.
    - Line height inside cells: Use line spacing that avoids a cramped feel and prevents ascenders/descenders from looking clipped. If a cell feels tight, adjust wrapping/width/padding before shrinking type.
    - Width + wrapping sanity check: Avoid default equal-width columns when the content in each column clearly has different sizes. Avoid lines that run so close to the right edge that the cell feels overfull. If this happens, prefer wrapping or column-width adjustments before reducing font size.
    - Spacing around tables: Keep clear separation between tables and surrounding text (especially the paragraph immediately above/below) so the layout doesn't feel stuck together. Captions and tables should stay visually paired, with deliberate spacing.
    - Quick visual QA pass: Look for text that appears "boundary-hugging", specifically content pressed against the top or left edge of a cell or sitting too close beneath a table. Also watch for overly narrow descriptive columns and short-value columns whose contents feel awkwardly pinned. Correct these issues through padding, alignment, wrapping, or small column-width adjustments.
- Forms / questionnaires: Design these as a usable form, not a spreadsheet.
  - Prioritize clear response options, obvious and well-sized check targets, readable scale labels, generous row height, clear section hierarchy, light visual structure. Please size fields and columns based on the content they hold rather than by equal-width table cells.
  - Use spacing, alignment, and subtle header/section styling to organize the page. Avoid dense full-grid borders, cramped layouts, and ambiguous numeric-only response areas.
- Coherence vs. fragmentation: In general, try to keep things to be one coherent representation rather than fragmented, if possible.
  - For example, don't split one logical dataset across multiple independent tables unless there's a clear, labeled reason.
  - For example, if a table must span across pages, continue to the next page with a repeated header and consistent column order
- Background shapes/colors: Where helpful, consider section bands, note boxes, control grids, or other visual containers with suitable colors to improve scanability and communication. Use them when they suit the document type. If you do use these, make sure they are formatted well, with no overlaps, awkward spacing, etc.
- Spacing: Please check rigorously for spacing issues. Please always use a natural amount of spacing between adjacent components. Use clear, generous vertical spacing between sections and paragraphs, and leave a bit of extra space between subheadings and the content that follows when it improves readability. Use indentation and alignment intentionally so the document's hierarchy is immediately clear. At the same time, avoid large "layout gaps" caused by a table or chart not fitting at the bottom of a page and getting pushed to the next one. If this happens, please try these suggestions:
  - scaling the visual modestly or simplify labels without hurting readability, formatting, or aesthetics of the visual
  - Splitting the table/figure cleanly across multiple pages, but use repeated headers to make the page continuation clear.
- Text boxes: For text boxes, please follow the same breathing-room rules as the tables: make sure to use generous internal padding, intentional alignment, and sufficient line spacing so text never feels cramped, clipped, or pinned to the edges. Keep spacing around the text box clear so it remains visually distinct from surrounding content, and if the content feels tight, prefer adjusting box size, padding, or text wrapping before reducing font size.
- Layout/archetype: Remember to choose the right document archetype/template (proposal, SOP, workflow, form, handbook, etc.). Use a coherent style system. Once a style system is chosen, apply it consistently across headings, spacing, table treatments, callouts, and accent usage. If appropriate to the document type, include a cover page or front-matter elements such as title, subtitle, metadata, or branding.

### Editing tasks (DOCX edits) — apply instead of major rewrite behavior

When the user asks to edit an existing document, preserve the original and make minimal, local changes:

- Prefer inline edits (small replacements) over rewriting whole paragraphs.
- Use clear inline annotations/comments at the point of change (margin comments or comment markers). Don’t move all feedback to the end.
- Keep the original structure unless there’s a strong reason; if a restructure is needed, do it surgically and explain via comments.
- Don’t “cross out everything and rewrite”; avoid heavy, blanket deletions. The goal is trackable improvements, not a fresh draft unless explicitly requested.

## Quick start (common one-liners)

```bash
# 0) Sanitize Google Docs-targeted title blocks before render/import
python scripts/google_docs_title_sanitize.py input.docx --out sanitized.docx
python scripts/google_docs_title_sanitize.py sanitized.docx --check

# 1) Render any DOCX to PNGs (visual QA)
python render_docx.py input.docx --output_dir out

# 2) Remove reviewer comments (finalization)
python scripts/comments_strip.py input.docx --out no_comments.docx

# 3) Accept tracked changes (finalization)
python scripts/accept_tracked_changes.py input.docx --mode accept --out accepted.docx

# 4) Accessibility audit (+ optional safe fixes)
python scripts/a11y_audit.py input.docx
python scripts/a11y_audit.py input.docx --out_json a11y_report.json
python scripts/a11y_audit.py input.docx --fix_image_alt from_filename --out a11y_fixed.docx

# 5) Redact sensitive text (layout-preserving by default)
python scripts/redact_docx.py input.docx redacted.docx --emails --phones
```

## Package layout

This skill is organized for progressive discovery: start here, then jump into task- or OOXML-specific docs.

DOCS SKILL PACKAGE

Root:
- SKILL.md: short overview + routing
- manifest.txt: machine-readable list of files to download (one relative path per line)
- render_docx.py: canonical DOCX→PNG renderer (container-safe LO profile + writable HOME + verbose logs)

References:
- references/design_presets.md: preset-first design tokens, archetype aliases, OOXML conversions, and preset audit checklist
- references/header_templates.md: concise first-page header pattern picker and code snippets

Tasks:
- tasks/read_review.md
- tasks/create_edit.md
- tasks/verify_render.md
- tasks/accessibility_a11y.md
- tasks/comments_manage.md
- tasks/protection_restrict_editing.md
- tasks/privacy_scrub_metadata.md
- tasks/multi_doc_merge.md
- tasks/style_lint_normalize.md
- tasks/forms_content_controls.md
- tasks/captions_crossrefs.md
- tasks/redaction_anonymization.md
- tasks/clean_tracked_changes.md
- tasks/compare_diff.md
- tasks/templates_style_packs.md
- tasks/watermarks_background.md
- tasks/footnotes_endnotes.md
- tasks/fixtures_edge_cases.md
- tasks/navigation_internal_links.md

OOXML:
- ooxml/tracked_changes.md
- ooxml/comments.md
- ooxml/hyperlinks_and_fields.md
- ooxml/rels_and_content_types.md

Troubleshooting:
- troubleshooting/libreoffice_headless.md
- troubleshooting/run_splitting.md

Scripts:

**Core building blocks (importable helpers):**
- `scripts/docx_ooxml_patch.py` — low-level OOXML patch helper (tracked changes, comments, hyperlinks, relationships). Other scripts reuse this.
- `scripts/fields_materialize.py` — materialize `SEQ`/`REF` field *display text* for deterministic headless rendering/QA.
- `scripts/table_geometry.py` — apply/audit exact Word table geometry for python-docx tables (`tblW`, `tblInd`, `tblGrid`, and every `tcW` match).

**High-leverage utilities (also importable, but commonly invoked as CLIs):**
- `render_docx.py` — canonical DOCX → PNG renderer (optional PDF via `--emit_pdf`; do not deliver intermediates unless asked).
- `scripts/render_and_diff.py` — render + per-page image diff between two DOCXs.
- `scripts/google_docs_title_sanitize.py` — deterministic OOXML sanitizer/audit for Google Docs-targeted DOCX title blocks; removes Word Title-style bottom borders/rules before render/import.
- `scripts/content_controls.py` — list / wrap / fill Word content controls (SDTs) for forms/templates.
- `scripts/captions_and_crossrefs.py` — insert Caption paragraphs for tables/figures + optional bookmarks around caption numbers.
- `scripts/insert_ref_fields.py` — replace `[[REF:bookmark]]` markers with real `REF` fields (cross-references).
- `scripts/internal_nav.py` — add internal navigation links (static TOC + Top/Bottom + figN/tblN jump links).
- `scripts/style_lint.py` — report common formatting/style inconsistencies.
- `scripts/style_normalize.py` — conservative cleanup (clear run-level overrides; optional paragraph overrides).
- `scripts/redact_docx.py` — layout-preserving redaction/anonymization.
- `scripts/privacy_scrub.py` — remove personal metadata + `rsid*` attributes.
- `scripts/set_protection.py` — restrict editing (read-only / comments / forms).
- `scripts/comments_extract.py` — extract comments to JSON (text, author/date, resolved flag, anchored snippets).
- `scripts/comments_strip.py` — remove all comments (final-delivery mode).

**Audits / conversions / niche helpers:**
- `scripts/fields_report.py`, `scripts/heading_audit.py`, `scripts/section_audit.py`, `scripts/images_audit.py`, `scripts/footnotes_report.py`, `scripts/watermark_audit_remove.py`
- `scripts/xlsx_to_docx_table.py`, `scripts/docx_table_to_csv.py`
- `scripts/insert_toc.py`, `scripts/insert_note.py`, `scripts/apply_template_styles.py`, `scripts/accept_tracked_changes.py`, `scripts/make_fixtures.py`

**v7 additions (stress-test helpers):**
- `scripts/watermark_add.py` — add a detectable VML watermark object into an existing header.
- `scripts/comments_add.py` — add multiple comments (by paragraph substring match) and wire up comments.xml plumbing if needed.
- `scripts/comments_apply_patch.py` — append/replace comment text and mark/clear resolved state (`w:done=1`).
- `scripts/add_tracked_replacements.py` — generate tracked-change replacements (`<w:del>` + `<w:ins>`) in-place.
- `scripts/a11y_audit.py` — audit a11y issues; can also apply simple fixes via `--fix_table_headers` / `--fix_image_alt`.
- `scripts/flatten_ref_fields.py` — replace REF/PAGEREF field blocks with their cached visible text for deterministic rendering.

> `scripts/xlsx_to_docx_table.py` also marks header rows as repeating headers (`w:tblHeader`) to improve a11y and multi-page tables.

Examples:
- examples/end_to_end_smoke_test.md

> Note: `manifest.txt` is **machine-readable** and is used by download tooling. It must contain only relative file paths (one per line).


## Coverage map (scripts ↔ task guides)

This is a quick index so you can jump from a helper script to the right task guide.

### Layout & style
- `style_lint.py`, `style_normalize.py` → `tasks/style_lint_normalize.md`
- `apply_template_styles.py` → `tasks/templates_style_packs.md`
- `section_audit.py` → `tasks/sections_layout.md`
- `heading_audit.py` → `tasks/headings_numbering.md`

### Figures / images
- `images_audit.py`, `a11y_audit.py` → `tasks/images_figures.md`, `tasks/accessibility_a11y.md`
- `captions_and_crossrefs.py` → `tasks/captions_crossrefs.md`

### Tables / spreadsheets
- `table_geometry.py` → root `Design Preset Contract` table geometry rules
- `xlsx_to_docx_table.py` → `tasks/tables_spreadsheets.md`
- `docx_table_to_csv.py` → `tasks/tables_spreadsheets.md`

### Fields & references
- `fields_report.py`, `fields_materialize.py` → `tasks/fields_update.md`
- `insert_ref_fields.py`, `flatten_ref_fields.py` → `tasks/fields_update.md`, `tasks/captions_crossrefs.md`
- `insert_toc.py` → `tasks/toc_workflow.md`

### Review lifecycle (comments / tracked changes)
- `add_tracked_replacements.py`, `accept_tracked_changes.py` → `tasks/clean_tracked_changes.md`
- `comments_add.py`, `comments_extract.py`, `comments_apply_patch.py`, `comments_strip.py` → `tasks/comments_manage.md`

### Privacy / publishing
- `privacy_scrub.py` → `tasks/privacy_scrub_metadata.md`
- `redact_docx.py` → `tasks/redaction_anonymization.md`
- `watermark_add.py`, `watermark_audit_remove.py` → `tasks/watermarks_background.md`

### Navigation & multi-doc assembly
- `internal_nav.py` → `tasks/navigation_internal_links.md`
- `merge_docx_append.py` → `tasks/multi_doc_merge.md`

### Forms & protection
- `content_controls.py` → `tasks/forms_content_controls.md`
- `set_protection.py` → `tasks/protection_restrict_editing.md`

### QA / regression
- `render_and_diff.py`, `render_docx.py` → `tasks/compare_diff.md`, `tasks/verify_render.md`
- `make_fixtures.py` → `tasks/fixtures_edge_cases.md`
- `docx_ooxml_patch.py` → used across guides for targeted patches

## Skill folder contents
- `tasks/` — task playbooks (what to do step-by-step)
- `references/` — compact reference material loaded only when needed, including design presets
- `ooxml/` — advanced OOXML patches (tracked changes, comments, hyperlinks, fields)
- `scripts/` — reusable helper scripts
- `examples/` — small runnable examples

## Default workflow (80/20)

**Rule of thumb:** every meaningful edit batch must end with a render + PNG review. No exceptions.
"80/20" here means: follow the simplest workflow that covers *most* DOCX tasks reliably.

**Golden path (don’t mix-and-match unless debugging):**
1. **Author/edit with `python-docx`** (paragraphs, runs, styles, tables, headers/footers).
2. **Render → inspect PNGs immediately** (DOCX → PNGs). Treat this as your feedback loop.
3. **Fix and repeat** until the PNGs are visually perfect.
4. **Only if needed**: use OOXML patching for tracked changes, comments, hyperlinks, or fields.
5. **Re-render and inspect again** after *any* OOXML patch or layout-sensitive change.
6. **Deliver only after the latest PNG review passes** (all pages, 100% zoom).

## Visual review (recommended)
Use the packaged renderer (dedicated LibreOffice profile + writable HOME):

```bash
python render_docx.py /mnt/data/input.docx --output_dir /mnt/data/out
# If debugging LibreOffice:
python render_docx.py /mnt/data/input.docx --output_dir /mnt/data/out --verbose
# Optional: also write <input_stem>.pdf to --output_dir (for debugging/archival):
python render_docx.py /mnt/data/input.docx --output_dir /mnt/data/out --emit_pdf
```

Then inspect the generated `page-<N>.png` files.

**Success criteria (render + visual QA):**
- PNGs exist for each page
- Page count matches expectations
- **Inspect every page at 100% zoom** (no “spot check” for final delivery)
- No clipping/overlap, no broken tables, no missing glyphs, no header/footer misplacement

**Note:** LibreOffice sometimes prints scary-looking stderr (e.g., `error : Unknown IO error`) even when output is correct. Treat the render as successful if the PNGs exist and look right (and if you used `--emit_pdf`, the PDF exists and is non-empty).

### What rendering does and doesn’t validate

- **Great for:** layout correctness, fonts, spacing, tables, headers/footers, and whether **tracked changes** visually appear.
- **Not reliable for:** **comments** (often not rendered in headless PDF export). For comments, also do **structural checks** (comments.xml + anchors + rels + content-types).

## Quality reminders
- Don’t ship visible defects (clipped/overlapping text, broken tables, unreadable glyphs).
- Don’t leak tool citation tokens into the DOCX (convert them to normal human citations).
- Prefer ASCII punctuation (avoid exotic Unicode hyphens/dashes that render inconsistently).

## Where to go next
- If the task is **reading/reviewing**: `tasks/read_review.md`
- If the task is **creating/editing**: `tasks/create_edit.md`
- If you need an **accessibility audit** (alt text, headings, tables, links): `tasks/accessibility_a11y.md`
- If you need to **extract or remove comments**: `tasks/comments_manage.md`
- If you need to **restrict editing / make read-only**: `tasks/protection_restrict_editing.md`
- If you need to **scrub personal metadata** (author/rsid/custom props): `tasks/privacy_scrub_metadata.md`
- If you need to **merge/append DOCXs**: `tasks/multi_doc_merge.md`
- If you need **format consistency / style cleanup**: `tasks/style_lint_normalize.md`
- If you need **forms / content controls (SDTs)**: `tasks/forms_content_controls.md`
- If you need **captions + cross-references**: `tasks/captions_crossrefs.md`
- If you need **redaction/anonymization**: `tasks/redaction_anonymization.md`
- If the task is **verification/raster review**: `tasks/verify_render.md`
- If your render looks wrong but content is right (stale fields): `tasks/fields_update.md`
- If you need a **Table of Contents**: `tasks/toc_workflow.md`
- If you need **internal navigation links** (static TOC + Back-to-TOC + Top/Bottom): `tasks/navigation_internal_links.md`
- If headings/numbering/TOC levels are messy: `tasks/headings_numbering.md`
- If you have mixed portrait/landscape or margin weirdness: `tasks/sections_layout.md`
- If images shift or overlap across renderers: `tasks/images_figures.md`
- If you need spreadsheet ↔ table round-tripping: `tasks/tables_spreadsheets.md`
- If you need **tracked changes (redlines)**: `ooxml/tracked_changes.md`
- If you need **comments**: `ooxml/comments.md`
- If you need **hyperlinks/fields/page numbers/headers**: `ooxml/hyperlinks_and_fields.md`
- If LibreOffice headless is failing: `troubleshooting/libreoffice_headless.md`
- If you need a **clean copy** with tracked changes accepted: `tasks/clean_tracked_changes.md`
- If you need to **diff two DOCXs** (render + per-page diff): `tasks/compare_diff.md`
- If you need **templates / style packs (DOTX)**: `tasks/templates_style_packs.md`
- If you need a **first-page header / cover / title block**: `references/header_templates.md`
- If you need **watermark audit/removal**: `tasks/watermarks_background.md`
- If you need **true footnotes/endnotes**: `tasks/footnotes_endnotes.md`
- If you want reproducible fixtures for edge cases: `tasks/fixtures_edge_cases.md`

## Codex App final response citations

When summarizing document work in Codex App, cite only the final delivered DOCX.

Use page citations when page numbers come from the latest rendered or inspected final document:

```text
::codex-file-citation{path="/abs/path/file.docx" artifact_kind="document" page_number="4"}
```

Document citations navigate by page only. Do not add object IDs, labels, paragraph IDs, table IDs, or cell IDs. If page numbers are not reliable, use a plain file citation or omit page-specific citations rather than guessing.

Do not cite internal PNG renders, PDFs, source notes, scratch files, builders, or QA outputs unless asked.
