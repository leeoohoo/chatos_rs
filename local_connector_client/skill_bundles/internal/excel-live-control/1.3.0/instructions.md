# ChatOS Excel Live Control

Use this Skill only for Microsoft Excel workbooks that the user has already opened in the current desktop session. It is separate from the file-based Spreadsheets Skill and must never silently switch to file-based workbook processing.

## Release 1.3.0 scope

This release keeps the no-launch discovery and bounded range reads from `1.2.0` and adds approval-gated, exact-snapshot-bound cell-content replacement. It can:

- inspect whether Microsoft Excel is installed and already running;
- list bounded metadata for up to 32 already-open workbooks;
- return opaque `workbook_id` and `worksheet_id` values bound to the current Excel process and private full-name identity;
- read one exact canonical uppercase A1 range of at most 256 cells with `excel_read_range`;
- return a `range_snapshot_id` bound to the exact workbook, worksheet, range geometry, and normalized cell state;
- after a fresh read and mandatory interactive approval, replace the exact same range with typed blank, boolean, bounded number, bounded text, or strictly allowlisted local formula cells through `excel_write_range`;
- verify the write in the platform bridge and again in Core, or attempt to restore and verify the exact pre-write target range after a partial write or verification failure.

The private workbook full-name identity and complete expected cell snapshot are used only for identity and stale-state verification. They travel to the macOS JXA or Windows PowerShell bridge through stdin and are never placed in process arguments. Approval arguments contain only opaque IDs, range geometry, cell counts, text-character count, snapshot ID, and a SHA-256 content digest; cell text and formulas are not stored in the approval arguments.

## Safety rules

- Never launch, activate, select, close, save, export, reopen, explicitly calculate, or change calculation mode in Microsoft Excel.
- `excel_write_range` is published only by the signed bundled Plugin runtime when interactive approval is available. Never bypass, cache, or reuse an approval decision.
- Before a write, call `excel_read_range` for the exact intended workbook, worksheet, and range. Pass its exact `range_snapshot_id`. A stale ID, identity drift, geometry drift, or any cell-state drift fails before mutation.
- Writes are limited to one visible, unprotected worksheet in a writable workbook and one canonical uppercase `A1` or `A1:B2` range of at most 256 cells. Hidden, very-hidden, protected, read-only, merged, detectably commented, or array-formula cells are rejected.
- The `cells` matrix must exactly match the range geometry. Use explicit `{ "kind": "blank" }`, `{ "kind": "value", "value": ... }`, or `{ "kind": "formula", "formula": "=..." }` cells.
- Text and formulas are limited to 128 characters. Empty text must use `blank`. Text whose first non-space character is `=`, `+`, `-`, `@`, or `'` is rejected to prevent formula interpretation. Numeric constants are finite and bounded to an absolute value of `1e15`.
- Formula text must start with `=` and use ASCII local syntax. Only `ABS`, `AND`, `AVERAGE`, `COUNT`, `COUNTA`, `IF`, `MAX`, `MIN`, `NOT`, `OR`, `ROUND`, and `SUM` are allowed. Cell/worksheet references, booleans, and numeric expressions are allowed; strings, named ranges, structured references, external workbooks, URLs, UNC/drive paths, DDE, macros, dynamic-data functions, and non-allowlisted functions are rejected.
- A target containing truncated text, truncated formulas, hidden formulas, external formulas, unsupported scalar values, or formulas outside the same rollback allowlist is not writable because exact restoration cannot be proven.
- The bridge revalidates process, workbook index/name/private identity, workbook writable state, worksheet index/name/visibility/protection, exact range geometry, and every expected cell before writing. It reads back every target cell after writing. Core then revalidates the full workbook snapshot and independently reads the range again before reporting success.
- If a write or its first readback fails after mutation begins, the bridge attempts to restore every target cell from the exact pre-write snapshot and reads the range again. Report a normal tool error even when rollback is verified; do not automatically retry. A bridge timeout, process crash, malformed result, concurrent user edit, or rollback mismatch means complete rollback is not proven: tell the user to inspect the exact range before any retry.
- Rollback covers only the target cells' values or formulas. It is not a workbook transaction and does not undo Excel's normal automatic recalculation of dependent formulas elsewhere. ChatOS never calls a calculate API.
- This release does not write formatting, names, worksheet/workbook state, charts, tables, pivots, comments, VBA, links, shapes, other objects, or workbook file bytes, and it never saves or exports the workbook.
- Bridge execution is bounded to 8 seconds for reads, 20 seconds for writes, and 512 KiB of output. If macOS Automation permission is denied, ask the user to allow ChatOS to control Microsoft Excel in System Settings; do not bypass the decision with UI scripting.
- If Excel is not running, report that state. Do not start it automatically.

## Tool sequence

1. Call `excel_live_status`. A running supported instance reports `ready`; writes still require the approval-gated tool to be present.
2. Call `excel_list_open_workbooks`, choose the intended workbook without assuming the active workbook is correct, and retain its exact `workbook_id`.
3. Call `excel_inspect_workbook` and retain the exact `worksheet_id` for the intended visible, unprotected worksheet.
4. Call `excel_read_range` with the exact IDs and canonical range. Review every returned cell and retain the exact `range_snapshot_id`.
5. For a write, submit the same IDs and range, that fresh snapshot ID, and an exact typed cell matrix to `excel_write_range`. The user must approve the summarized write each time.
6. Treat a successful result as verified target-cell content replacement only. It does not mean the workbook was saved. If the operation reports stale state, rediscover and re-read. If it reports uncertain rollback, stop and ask the user to inspect the range.

This release does not complete live workbook editing. Formatting, search, charts, tables, pivots, objects, save/export, full workbook transactions, visual verification, and real Windows/macOS write acceptance testing remain later work.
