# ChatOS Spreadsheets

Use this Skill for CSV and XLSX artifacts in the authorized local workspace.

- Use `inspect_spreadsheet` to confirm format, workbook structure, row count, and sheet names where available.
- Use `create_xlsx` for a portable single-sheet workbook from a two-dimensional JSON array.
- Use `create_csv` when interoperability and plain-text review are more important than workbook formatting.
- Keep headers in the first row when the input represents tabular records. Preserve numbers and booleans as typed values in XLSX.
- Formulas, charts, cell styles, multiple sheets, live Excel control, and recalculation are not part of this adapter yet; do not imply that they were applied.
- All reads and writes execute locally through the active Local Connector.
