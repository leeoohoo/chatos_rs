# Style and Formatting Instructions

## Formatting requirements when provided a formatted spreadsheet as part of the task
- If editing an uploaded/template workbook: render first, preserve and match existing style unless user asks to restyle.
- Before editing, inspect all relevant current styling attributes (fills, fonts, borders, merged cells, number formats). If changing values only, never overwrite or clear cell formats.

## Formatting Baseline
Apply these defaults to all spreadsheet outputs but ensure user provided style reference(s), template, or explicit formatting instructions take precedence. If the user specifies a style or reference, match that instead.

### Use a visually clear layout
- Headers should be formatted differently from data and derived cells to distinguish them with a consistent visual styel
- Use fill colors, borders, and merged cells judiciously to give the spreadsheet a professional visual style with a clear layout without overdoing it
- Set appropriate row heights and column widths to give a clean visual appearance; contents of cells should be readable within the cell, without excessive buffer space
- Keep row heights consistent within each section unless wrapped content requires expansion.
- Do not apply borders around every filled cell. Borders clarify structure, not decorate arbitrary ranges. Prefer thin, light borders for structure; use stronger borders only for important section breaks.
- Keep formatting and colors consistent for a given role or section for legibility, and ideally across the workbook. Conditional Formatting is preferred over manually painted cells when applying styles consistently over a range, column or table.
- Colors should be used to convey meaning.
- Use strategic whitespace or slightly taller section/header rows to separate sections
- Add data validation for editable categorical columns (`Status`, `Priority`, `Owner`) where feasible.
- When text wraps, prefer widening the column before allowing deep multi-line rows; if wrapping is necessary, increase row height just enough to fully show the content.
- Maintain structural elements (filters, tables, totals rows), and never introduce merged cells in calculation areas.
- If users are likely to edit the workbook after export, make worksheet cells the live source of truth: any downstream values or visual states that depend on editable inputs should be driven by formulas referencing worksheet cells, then styled with conditional formatting or presentation-only formatting instead of Python-precomputed values or one-time manual fills.
- Use freeze rows/columns when they help with usability, and especially for large datasets that span a lot of rows or columns. Follow best practices.
- Apply global styling only to populated or intentionally reserved ranges. Avoid formatting hundreds of unused rows or columns because this can expand the used range and degrade rendering, printing, and navigation.

### Avoiding gridlines
Define structure with explicit fills and borders rather than relying on default gridlines. Use subtle internal borders for separation and slightly stronger outside borders to frame sections or cards. Hide gridlines when explicit section styling already defines the sheet.

### Align and format by data type
Apply semantic formatting to entire columns or blocks:
- text/descriptive fields left-aligned
- labels centered or left-aligned depending on context
- numeric and currency fields right-aligned
- dates with explicit date formats
- financial values with explicit currency/accounting formats
- do not leave important numeric fields in raw General format

### Typography and whitespace
- Use bold sparingly and only to establish reading order.
- Give titles, summaries, and section breaks visible breathing room so the sheet does not feel cramped.

#### Use typography intentionally but conservatively
Use one display-style font choice for titles/section headers and one neutral readable font for body content when supported by the workbook viewer. Keep body text modest in size, reserve larger fonts for titles or major section labels, and avoid mixing many fonts or excessive emphasis.

### Prefer visible summaries over buried totals
Important totals should usually appear in a visible summary area near the top or in a side panel, even if table-footer totals also exist. Use formulas, not hardcoded values, and style summary cards as distinct panels with their own fill and border treatment.
