# ChatOS Excel Live Control

Use this Skill only for Microsoft Excel workbooks that the user has already opened in the current desktop session. It is separate from the file-based Spreadsheets Skill and must never silently switch to file-based workbook processing.

## Release 1.4.0 scope

This release keeps the no-launch discovery, bounded reads, and approval-gated cell-content replacement from `1.3.0`. It adds approval-gated, exact-snapshot-bound number-format replacement. It can:

- inspect whether Microsoft Excel is installed and already running;
- list bounded metadata for up to 32 already-open workbooks;
- return opaque `workbook_id` and `worksheet_id` values bound to the current Excel process and private full-name identity;
- read one exact canonical uppercase A1 range of at most 256 cells with `excel_read_range`;
- return a `range_snapshot_id` bound to the exact workbook, worksheet, range geometry, normalized cell contents, and private bounded number-format identity;
- expose only a safe public number-format classification: `general`, `integer`, `decimal_2`, `percent_2`, `date`, `datetime`, `text`, or custom/unavailable metadata; arbitrary custom format text is not returned to the model;
- after a fresh read and mandatory interactive approval, replace exact cell contents through `excel_write_range`, or apply one fixed number-format preset to the exact range through `excel_set_number_format`;
- verify content writes preserve number formats, verify format writes preserve values and formulas, and attempt exact target-range rollback after a partial mutation or verification failure.

The private workbook full-name identity, complete expected cell snapshot, and bounded raw number-format identity are used only for identity, stale-state, preservation, and rollback verification. They travel to the macOS JXA or Windows PowerShell bridge through stdin and are never placed in process arguments or public tool results. Approval arguments contain only opaque IDs, range geometry, cell counts, snapshot ID, the selected format preset, and—for content writes—a SHA-256 content digest plus bounded summary counts; cell text, formulas, private paths, and custom format text are not stored in approval arguments.

## Safety rules

- Never launch, activate, select, close, save, export, reopen, explicitly calculate, or change calculation mode in Microsoft Excel.
- `excel_write_range` and `excel_set_number_format` are published only by the signed bundled Plugin runtime when interactive approval is available. Never bypass, cache, or reuse an approval decision.
- Before either write, call `excel_read_range` for the exact intended workbook, worksheet, and range. Pass its exact `range_snapshot_id`. A stale ID, identity drift, geometry drift, content drift, or number-format drift fails before mutation.
- Writes are limited to one visible, unprotected worksheet in a writable workbook and one canonical uppercase `A1` or `A1:B2` range of at most 256 cells. Hidden, very-hidden, protected, read-only, merged, detectably commented, or array-formula cells are rejected.
- Content writes keep the strict typed blank/value/formula matrix and formula allowlist from `1.3.0`. Text/formulas remain limited to 128 characters, formula-like text is rejected, numeric constants are finite and at most `1e15`, and only local ASCII formulas using `ABS`, `AND`, `AVERAGE`, `COUNT`, `COUNTA`, `IF`, `MAX`, `MIN`, `NOT`, `OR`, `ROUND`, and `SUM` are accepted.
- Number-format writes accept exactly one of seven fixed presets: `general`, `integer`, `decimal_2`, `percent_2`, `date`, `datetime`, or `text`. Never accept or synthesize an arbitrary Excel format string.
- A target containing truncated values, displayed text, formulas, or number-format identity; unavailable number-format identity; hidden formulas; or external formulas is not writable because exact stale-state or rollback verification cannot be proven. Content replacement also keeps the `1.3.0` rollback allowlist requirement for existing values and formulas.
- The bridge revalidates process, workbook index/name/private identity, workbook writable state, worksheet index/name/visibility/protection, exact range geometry, every expected cell, and the private number-format identity before writing. It reads every target cell after writing. Core then revalidates the full workbook snapshot and independently reads the range again before reporting success.
- A content write must preserve every target cell's exact number format. A number-format write must preserve every target value/formula state; displayed text may change as the intended consequence of formatting.
- If mutation or first readback fails after mutation begins, the bridge attempts to restore the exact target contents or exact prior number formats and verifies the complete prior snapshot. Report a normal tool error even when rollback is verified; do not automatically retry. A bridge timeout, process crash, malformed result, concurrent user edit, or rollback mismatch means complete rollback is not proven: tell the user to inspect the exact range before any retry.
- Rollback covers only target cell contents for `excel_write_range` or target cell number formats for `excel_set_number_format`. It is not a workbook transaction and does not undo Excel's normal automatic recalculation of dependent formulas elsewhere. ChatOS never calls a calculate API.
- This release does not change fonts, fills, borders, alignment, dimensions, conditional formatting, names, worksheet/workbook state, charts, tables, pivots, comments, VBA, links, shapes, other objects, or workbook file bytes, and it never saves or exports the workbook.
- Bridge execution is bounded to 8 seconds for reads, 20 seconds for writes, and 512 KiB of output. If macOS Automation permission is denied, ask the user to allow ChatOS to control Microsoft Excel in System Settings; do not bypass the decision with UI scripting.
- If Excel is not running, report that state. Do not start it automatically.

## Tool sequence

1. Call `excel_live_status`. A running supported instance reports `ready`; writes still require approval-gated tools to be present.
2. Call `excel_list_open_workbooks`, choose the intended workbook without assuming the active workbook is correct, and retain its exact `workbook_id`.
3. Call `excel_inspect_workbook` and retain the exact `worksheet_id` for the intended visible, unprotected worksheet.
4. Call `excel_read_range` with the exact IDs and canonical range. Review every returned cell and retain the exact `range_snapshot_id`.
5. For content replacement, submit the same IDs/range, fresh snapshot ID, and exact typed matrix to `excel_write_range`. For number formatting, submit the same IDs/range, fresh snapshot ID, and one fixed preset to `excel_set_number_format`. The user must approve each summarized mutation.
6. Treat success as verified target-cell content or number-format mutation only. It does not mean the workbook was saved. If the operation reports stale state, rediscover and re-read. If it reports uncertain rollback, stop and ask the user to inspect the range.

This release does not complete live workbook editing. Rich cell styling, conditional formatting, search, charts, tables, pivots, objects, save/export, full workbook transactions, visual verification, and real Windows/macOS write acceptance testing remain later work.
