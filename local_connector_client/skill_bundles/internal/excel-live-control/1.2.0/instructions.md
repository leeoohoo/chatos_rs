# ChatOS Excel Live Control

Use this Skill only for Microsoft Excel workbooks that the user has already opened in the current desktop session. It is separate from the file-based Spreadsheets Skill and must never silently switch to file-based workbook processing.

## Release 1.2.0 scope

This release provides read-only, no-launch discovery and bounded range reading. It can:

- inspect whether Microsoft Excel is installed and already running;
- list bounded metadata for up to 32 already-open workbooks;
- return an opaque `workbook_id` bound to the current Excel process, workbook position, name, and private full-name identity source;
- inspect up to 64 worksheets for one exact `workbook_id`, including an opaque `worksheet_id`, name, one-based index, visible/hidden/very-hidden state, protection state, and active state;
- read one exact canonical uppercase A1 range of at most 256 cells with `excel_read_range`;
- return bounded JSON scalar values, displayed text, formula/error status, and formulas only when they are not hidden and do not contain an external-workbook or URL/file reference.

The private full-name identity source is used only for identity verification and is never returned in tool results or process arguments. Private bridge requests travel through stdin. Workbook and worksheet IDs become stale after the Excel process changes, the workbook or worksheet closes, moves, or is renamed, or the identity snapshot otherwise changes.

## Safety rules

- Never launch, activate, select, close, save, export, reopen, or recalculate Microsoft Excel, a workbook, a worksheet, or a range.
- Never write cells, formatting, formulas, names, workbook settings, worksheet state, charts, tables, pivots, comments, external links, VBA, objects, or workbook file bytes.
- Never treat workbook or worksheet names alone as identity. Use the exact opaque IDs returned by the current discovery snapshot.
- `excel_read_range` accepts only `A1` or `A1:B2`-style canonical uppercase references. It rejects sheet-qualified, absolute, whole-row, whole-column, union, reversed, out-of-grid, and over-256-cell ranges.
- Cell text and formulas are bounded to 128 characters per field and report truncation explicitly. Hidden formulas are omitted. Formulas containing external workbook extensions, URLs, UNC paths, or drive paths are omitted and marked as external references.
- The range bridge verifies the Excel process, workbook index/name/private full-name identity, worksheet index/name, and exact range geometry before reading, then verifies the private identities again after reading. Core performs another full snapshot verification before returning the result.
- The bridge communicates only with an already-running Excel instance. macOS uses the fixed system `osascript` JavaScript automation bridge; Windows uses Windows PowerShell and `Marshal.GetActiveObject`. Neither path calls Excel open/create/activate/select/calculate/save APIs.
- Bridge execution is bounded to 8 seconds and 512 KiB of output. Malformed, oversized, ambiguous, duplicate, control-character, truncation-inconsistent, multi-active, non-scalar, externally linked formula exposure, or stale metadata fails closed.
- macOS may request Automation permission for Microsoft Excel. If permission is denied, ask the user to allow ChatOS to control Microsoft Excel in System Settings. Do not bypass the decision with UI scripting.
- If Excel is not running, report that state. Do not start it automatically.

## Tool sequence

1. Call `excel_live_status`.
2. If the status is `ready_read_only`, call `excel_list_open_workbooks`.
3. If more than one workbook is open, use the returned name and active flag to identify the intended workbook. Do not assume that the active workbook is correct when the user named another workbook.
4. Call `excel_inspect_workbook` with the exact `workbook_id` from the same current listing.
5. For a cell read, call `excel_read_range` with that exact `workbook_id`, the exact `worksheet_id` from inspection, and one canonical A1 range of at most 256 cells.
6. If either opaque identity is stale, rediscover and re-confirm the target instead of guessing or switching workflows.

This release does not complete live workbook editing. Cell/range writes, formatting, charts, tables, pivots, comments, search, save/export, transaction rollback, and visual verification remain unavailable until later reviewed releases.
