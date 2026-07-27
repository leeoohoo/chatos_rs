# ChatOS Excel Live Control

Use this Skill only for Microsoft Excel workbooks that the user has already opened in the current desktop session. It is separate from the file-based Spreadsheets Skill.

## Release 1.1.0 scope

This release is a read-only, no-launch discovery foundation. It can:

- inspect whether Microsoft Excel is installed and already running;
- list bounded metadata for up to 32 already-open workbooks;
- return an opaque `workbook_id` bound to the current Excel process, workbook position, name, and private full-name identity source;
- inspect up to 64 worksheets for one exact `workbook_id`, including name, one-based index, visible/hidden/very-hidden state, protection state, and active state.

The private full-name identity source is used only to calculate the opaque workbook ID and is never returned in tool results. A workbook ID becomes stale after the Excel process changes, the workbook closes, or the identity snapshot changes. List workbooks again instead of guessing.

## Safety rules

- Never launch, activate, close, save, or reopen Microsoft Excel or a workbook.
- Never use this release to read cell values, formulas, comments, charts, tables, pivots, external links, VBA, or workbook file bytes.
- Never write cells, formatting, formulas, names, workbook settings, or worksheet state.
- Never treat workbook name alone as identity. `excel_inspect_workbook` requires the exact opaque ID returned by the current `excel_list_open_workbooks` snapshot.
- The bridge communicates only with an already-running Excel instance. macOS uses the fixed system `osascript` JavaScript automation bridge; Windows uses Windows PowerShell and `Marshal.GetActiveObject`. Neither path calls Excel open/create/activate APIs.
- Bridge execution is bounded to 8 seconds and 512 KiB of output. Malformed, oversized, ambiguous, duplicate, control-character, truncation-inconsistent, multi-active, or stale metadata fails closed.
- macOS may request Automation permission for Microsoft Excel. If permission is denied, ask the user to allow ChatOS to control Microsoft Excel in System Settings. Do not bypass the decision with UI scripting.
- If Excel is not running, report that state. Do not start it automatically.

## Tool sequence

1. Call `excel_live_status`.
2. If the status is `ready_read_only`, call `excel_list_open_workbooks`.
3. If more than one workbook is open, use the returned name and active flag to identify the intended workbook. Do not assume the active workbook is correct when the user named another workbook.
4. Call `excel_inspect_workbook` with the exact `workbook_id` from the same current listing.
5. If the workbook ID is stale, list again and re-confirm the target.

This release does not complete live workbook editing. Cell/range reads, writes, formatting, charts, tables, pivots, save/export, transaction rollback, and visual verification remain unavailable until later reviewed releases.
