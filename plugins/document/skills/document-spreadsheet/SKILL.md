---
name: document-spreadsheet
description: Read and modify XLSX workbooks with bounded ranges, typed values and formulas, deliberate sheet operations, and post-write validation.
metadata:
  chatos.role: leaf
---

# Spreadsheets

Inspect the workbook and read only the ranges needed for the task. `spreadsheet_read_range` returns typed values, formulas, and cached results; use it before writing when existing shape or formulas matter.

Use `spreadsheet_write_range` for one bounded rectangular matrix. `null` means leave the source cell unchanged; it is not a request to clear the cell. Use formula objects beginning with `=` rather than formula-looking strings.

Use `spreadsheet_manage_sheets` for add, rename, delete, color, visibility, and freeze-pane operations. Confirm exact worksheet names and avoid deleting or renaming a sheet that formulas or the user still depend on.

Every write produces a new artifact. Validate it and reread critical output ranges. Render selected worksheets only when visual layout, print preview, or presentation quality matters.

Read [spreadsheet examples](references/examples.md) before formula or sheet-structure changes.
