# ChatOS Spreadsheets

Use this Skill for bounded CSV and XLSX artifact work inside the authorized local workspace.

- Use `inspect_spreadsheet` before and after XLSX changes. XLSX inspection reports worksheet names, used row/column bounds, cell and formula counts, frozen rows, custom column-width counts, and whether full recalculation is requested on open.
- Use `create_xlsx` legacy `rows` mode for a simple single-sheet workbook. Use `worksheets` mode for up to 64 named sheets with per-sheet rows, up to 1000 frozen header rows, and explicit A–XFD column widths.
- XLSX cells preserve JSON strings, finite numbers, booleans, and nulls as typed values. A cell object may use `value` plus a built-in `number_format`, or `formula` plus an optional `cached_value` and `number_format`.
- Supported formats are `general`, `integer`, `decimal_2`, `percent_2`, `date`, and `datetime`. Non-general formats require numeric values; date and datetime inputs are Excel serial numbers rather than ISO strings.
- Formula text may include a leading equals sign, but is stored in normal OOXML formula form. Only `ABS`, `AND`, `AVERAGE`, `COUNT`, `COUNTA`, `IF`, `MAX`, `MIN`, `NOT`, `OR`, `ROUND`, and `SUM` are allowed. External-workbook syntax, string literals, dynamic network/link functions, and unsupported characters fail closed. Formula workbooks request full recalculation when opened.
- Use `update_xlsx_range` to write one non-empty rectangular two-dimensional value range to an exact worksheet and a top-left A1 reference. The source is never changed in place; `target_path` must be distinct. Unchanged ZIP entries and cells are preserved.
- Range updates refuse merged-cell intersections and existing shared, array, or data-table formula intersections. Standard default SpreadsheetML namespaces are supported for editing; prefixed namespace variants fail closed rather than producing an ambiguous workbook.
- Use `create_csv` when interoperability and plain-text review are more important than workbook structure or formatting. String cells whose first non-whitespace character is `=`, `+`, `-`, or `@`, or which begin with tab/newline control characters, are prefixed with an apostrophe to prevent spreadsheet formula injection; JSON numeric cells are not changed.
- A workbook is limited to 100000 requested cells, 100 MiB compressed and expanded artifact boundaries, 16 MiB XML parts, 10000 ZIP entries, Excel's 1048576-row/16384-column bounds, 32767 characters per text cell, and 4096 bytes per formula.
- Charts, pivot tables, macros, external links, automatic ISO-date conversion, visual rendering, Google Sheets handoff, and live Microsoft Excel control are not part of this release. Do not imply that they were applied.
- All reads and writes execute locally through the active Local Connector. No Office process, project service, network request, or fixed port is started by these tools.
