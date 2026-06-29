---
name: "Spreadsheets"
description: "Use this skill when a user requests to create, modify, analyze, visualize, or work with spreadsheet files (`.xlsx`, `.xls`, `.csv`, `.tsv`) or Google Sheets-targeted spreadsheet artifacts with formulas, formatting, charts, tables, and recalculation."
---

# Spreadsheets skill (Create • Edit • Analyze • Visualize)
Use this skill when you need to work with spreadsheets (.xlsx, .csv, .tsv) to do any of the following:
- Create or modify a new workbook/sheet with proper formulas, cell/number formatting, and structured layout
- Read or analyze tabular data (filter, aggregate, pivot, compute metrics) directly in a sheet
- Visualize data with in-sheet charts/tables and sensible formatting
- Recalculate/evaluate formulas to update results after changes

## Decision Boundary

- For Google Sheets-targeted outputs, such as creating or editing a Google Sheet, follow the additional instructions here: `routing/google_sheets.md`.

Do not follow those routing instructions if irrelevant to the task. Default is to create/edit spreadsheets with artifact tool.

## Tools + Contract Requirements
- Use `@oai/artifact-tool` JS library for all spreadsheet authoring, using only the executables and dependency paths provided by `load_workspace_dependencies`. Do not use system, global, or repo-local dependencies.
- If the runtime or `@oai/artifact-tool` is unavailable, report a blocker. Do not guess or search for paths, install packages, use resolution hacks, or import bundled internals.
- Work in a writable, conversation-specific or tmp directory. In that working directory, create a `node_modules` symlink or Windows junction pointing to the loader-provided `node_modules` directory. Never modify the loader-provided dependency directory.
- Prefer one executable `.mjs` builder and patch/rerun it. Do not use heredocs or duplicate builders.
- Use the provided API reference. Do not inspect package internals or prototypes. If blocked, run at most one targeted `workbook.help("<api_or_feature>")` query.
- Do not use alternate workbook creation/editing libraries such as `openpyxl`, `xlsxwriter`, or `pandas.ExcelWriter` unless the user explicitly asks.
- For supporting analysis or data processing outside workbook authoring, use JS or spreadsheet formulas when sufficient. If Python is necessary, prefer the bundled python libraries, save JSON/CSV intermediates, and have the JS builder create the workbook. Use existing system Python or user-provided libraries only when the bundled environment lacks a required capability. Keep auditable and user-editable calculations in the workbook as formulas.
- Use `update_plan` for complex spreadsheet work.

### Final Response
- Include a short user-visible summary and standalone Markdown link(s) only to final `.xlsx` artifact(s), one per line: `[Revenue Model - MNST.xlsx](/absolute/path/to/revenue_model_mnst.xlsx)`.
- Do not mention or link builders, previews, or other support files unless requested.

Other documents:
- `style_guidelines.md`: REQUIRED for formatting requirements
- `API_QUICK_START.md`: REQUIRED API documentation for `artifact_tool` JS library, which exposes methods to read, manipulate, edit, recalculate, render, import and save spreadsheets. You must read it entirely to get started.
- `charts.md`: Read when creating or editing charts.

## Domain Requirements
You must read these domain rules when the request clearly relates to the domain, but do not load domain guidance for unrelated tasks unless asked:
- Finance and investment banking: `domain_guidance/financial_models.md`
- Corporate finance and FP&A: `domain_guidance/corporate_finance_fpa.md`
- Healthcare: `domain_guidance/healthcare.md`
- Marketing and advertising: `domain_guidance/marketing_advertising.md`
- Scientific research: `domain_guidance/scientific_research.md`

Instruction precedance is as follows: user request > reference/template > domain/formatting defaults

## Making edits on a spreadsheet or using an uploaded reference or template.
- Before modifying: ALWAYS study and match the existing format, style and conventions when making edits by rendering and viewing the image. Read related values and formulas.
- For visual fix requests, start with the smallest plausible local change. Do not apply sheet-wide autofit, wrapping, or restyling unless requested.
- Ensure existing formulas, layouts, structures, and patterns are consistent. For example, if asked to add another column or row to a table and there is conditional formatting applied to the whole table, it should extend to the new column or rows as well.
- Keep edits targeted unless a broader change is clearly necessary. Exceptions are when there's dependencies, e.g. a dynamic chart that is based on the range of values in a table and a new row is added, the chart should also update.
- Extend conditional formatting if needed to keep style consistent for an area or table.
- Never overwrite formatting for spreadsheets with established formats, unless requested or to extend an added range.

## Importing or extracting data from screenshots or reference images
- When a reference image or screenshot is provided, use appropriate data formats (e.g. number/date formats) based on the workbook topic, audience and purpose instead of trying to recreate the rendered format with just text. Preserve numeric/date usability even when the screenshot shows locale-specific punctuation or currency symbols.
- Use formulas when appropriate and correct: For screenshot recreation, do not bulk-write numeric tables as all static values until you have separated any clearly formula-derived ranges; test adjacent numeric rows/columns for exact repeated relationships such as sums, differences, products, ratios, or constant multiples, then keep inputs hardcoded and write derived ranges as formulas.
- Match visible styling, but do not infer intentional formatting from ambiguous image artifacts such as zoom, antialiasing, or compression. Infer font weight only from relative contrast or clear semantics; if all visible text has the same apparent weight, use normal weight.

## Handling queries and questions
- The user may ask questions about the sheet instead of requesting an edit or a change. Simply answer those questions about the spreadsheet based on the context available rather than making an edit the user didn't intend for. You can use inspect to learn more or directly read values/formulas/tables etc via accessor methods.

## Error Recovery
On first API error:
1. Read error text.
2. Run one targeted `workbook.help("<exact_api>")` query only if needed.
3. Retry with minimal patch (not full rewrite).
4. Continue from existing workbook state.

Do not loop indefinitely on similar failures.

## Formula Rules
- Place assumptions and raw data in dedicated cells or clearly delineated input ranges, following the reference workbook's organization when one is provided.
- Derived values must be formulas (not hardcoded) and legible.
- Keep calculations formula driven, and prefer consistent formula patterns across a range where possible for readability. For example, formulas should be consistent across all projection periods.
- Use absolute/relative references correctly for fill/copy behavior.
- Use references instead of hardcoded or magic numbers inside formulas e.g. Use `=A5*(1+$A$6)` instead of =A5*1.05
- Formulas should be simple, legible and **easily auditable**. Use helper cells for intermediate values rather than performing complex calculations in a single cell. Users should be able to trace the model from inputs to outputs easily.
- No harcoded numbers inside calculation areas unless explicitly allowed. Always ensure color formatting conventions are properly applied.
- For any complex formulas or important assumptions, add comments to cells to explain.
- Always reference cells on other Excel sheets using the format ='Sheet Name'!A1, wrapping the sheet name in single quotes every time since quotes are required for any spaces or special characters.

### Ensure formulas are correct
- Checklist: No formula errors, all cell references are correct, no off-by-one errors in ranges, edge cases (zero values, negative numbers) are handled, no unintended circular references.

## Data Formatting Rules
- Store numbers, percentages, currency, and dates as typed spreadsheet values, not preformatted strings. Use text only for true identifiers such as ZIP codes, account IDs, SKUs, or labels.
- Use Excel-invariant number/date format codes, not locale-specific display strings. Examples include `#,##0`, `#,##0.0`, `0.0%`, `0.00%`, `"$"#,##0`, `"$"#,##0.00`, `yyyy-mm-dd`, `mmm yyyy` but choose the format that best fits the data.
- Percentages: When not specified or no reference is provided, use 1 decimal for most internal/analytical cells, 0 decimals for user-facing/dashboard outputs, and 2 decimals where small differences in rates matter.
- Do not swap `.` and `,` in format codes to mimic locale separators; separators are controlled by spreadsheet/render locale. Use `0.0%`, not `0,0%`, and `#,##0`, not `#.##0`.
- Choose the appropriate format for readability. Match precision to meaning: counts use `#,##0`; rates usually use `0.0%` or `0.00%`; currency uses whole units unless cents matter.

## Quality Guidelines
- Build correct, readable workbooks for the intended audience with clear structure, consistent formatting, reliable formulas, and useful outputs. Keep them as simple as practical.
- After autofit and wrapping, cap oversized column widths and row heights.
- Make workbooks easy for another person to update, trace, and audit without the original author.

## Completion Criteria
Complete only when:
- Workbook content is populated and formulas compute.
- No obvious formula errors in key scanned ranges (no bad refs/off-by-one/circular errors).
- `.xlsx` saved to `outputs/<unique_thread_id>/`.
- Visual render verification passes:
  - Layout is organized, legible, and aligned to request style (or default formatting baseline).
  - Important numbers and callouts are all visible.
  - Numbers, text, charts and content is not clipped or awkwardly wrapped.

## Verification Rules
Before final response, verify values/formulas and visual quality.

1. Inspect key ranges:
```js
const check = await workbook.inspect({
  kind: "table",
  range: "Dashboard!A1:H20",
  include: "values,formulas",
  tableMaxRows: 20,
  tableMaxCols: 12,
});
console.log(check.ndjson);
```

2. Scan formula errors:
```js
const errors = await workbook.inspect({
  kind: "match",
  searchTerm: "#REF!|#DIV/0!|#VALUE!|#NAME\\?|#N/A",
  options: { useRegex: true, maxResults: 300 },
  summary: "final formula error scan",
});
console.log(errors.ndjson);
```

3. Render sheets/ranges to verify visual output (skip if already verified and no style changes):
```js
const blob = await workbook.render({ sheetName: "Sheet1", range: "A1:H20", scale: 2 });
```
Make sure you do at least one visual pass of all the sheets in the workbook before the final export.

Visual requirements:
- Fix severe defects before finalizing: blank/broken charts, clipped key headers or numbers, unreadable colors, obvious formula errors, default blank sheets, or content outside the visible working area.
- Ensure logical labels or titles appear once, and merged ranges exist where labels or content intentionally span multiple columns.
- Ensure texts are all clearly visible and NOT clipped, columns and appropriately sized
- Do focused visual repair pass(s) after the initial render. Limit looping/time sinks for minor polish: stop once the workbook is correct, legible, and exported; note any minor limitation briefly and finalize.

4. Keep verification compact:
- Inspect key ranges.
- Avoid huge NDJSON dumps.

5. Export:
```js
await fs.mkdir(outputDir, { recursive: true });
const output = await SpreadsheetFile.exportXlsx(workbook);
await output.save(`${outputDir}/output.xlsx`);
```

6. Finalize immediately after successful export + compact verification.
- Do not export extra `.xlsx` variants unless asked.

## Citation Requirements
### Cite sources inside the spreadsheet
- Use plain-text URLs in spreadsheet cells.
- For financial models, cite model-input sources in cell comments.
- For researched row-wise data tables, include source URLs in a dedicated source column.

## Final response citations

When summarizing workbook content in Codex App, cite only the final delivered `.xlsx`.

Prefer a plain file citation for workbook, sheet, table, chart, image, or shape summaries:

```text
::codex-file-citation{path="/abs/path/book.xlsx"}
```

Use a workbook range citation only when selecting that A1 address in the opened workbook is useful:

```text
::codex-file-citation{path="/abs/path/book.xlsx" artifact_kind="workbook" range="A1:C4"}
```

Current Codex App workbook navigation honors `range` as the initial selected address, but does not yet honor `sheet`, and object-only citations are not navigable. Do not emit workbook citations that rely on `sheet` routing or only specify `object_id` for tables, charts, images, or shapes. Name the sheet/object in prose and cite a representative range only when that range target is reliable; otherwise use the plain file citation.

Do not cite previews, source notes, scratch files, generated JSON/CSV/logs, builders, or QA outputs unless asked.

## Comment Author
- If the authenticated/user profile or env context provides a user display name, use it as the threaded comment display name unless the user requests another name. Default to `User`.


## Source, PDF, and Attachment Processing
- Keep source notes compact: record file name, section/table label, and enough context to audit the number. Do not paste large PDF excerpts into the workbook unless requested.
- Bundled Python libraries available in the bundled runtime environment for extraction/analysis include `pandas`, `numpy`, `pypdf`, `python-docx`, and `reportlab`. You may read/extract in separate scripts if needed.
- Bundled JS libraries available for document/PDF work include `docx`, `pdf-lib`, and `pdfjs-dist`.
