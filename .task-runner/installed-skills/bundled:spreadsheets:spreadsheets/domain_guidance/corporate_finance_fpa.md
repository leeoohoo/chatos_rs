# Corporate Finance / FP&A Guidance

Use this domain guidance to make FP&A workbooks readable, refreshable, auditable, and decision-useful.

## Priority And Overrides

Additional requirements for corporate finance and FP&A workbooks apply only after higher-priority user intent is honored. If any specific rule conflicts with the user's request, prioritize the user's request first. If a reference workbook, template file, screenshot, or style guide is provided, prioritize matching that reference before applying these defaults.

This is domain guidance only. `Spreadsheets` and any relevant routing guidance remain authoritative for tool choice, workflow, verification, export, and final response behavior.

## When To Use

Use this guidance for:

- Budgets, forecasts, rolling forecasts, long-range plans, monthly business reviews, KPI packs, management reports, headcount plans, opex planning, revenue planning, cash planning, and operating plans.
- Requests like "make this FP&A-quality", "make this exec-ready", or "clean this up for a leadership review".
- Workbooks that must be refreshed regularly, shared across functions, or compiled from multiple department inputs.

Do not use this guidance for DCF, LBO, 3-statement, comps, or investment-banking models when `financial_models.md` is the better fit.

## Deliverable Standard

The workbook should be:

- Readable: clear visual hierarchy, obvious drivers, visible units, and logical reporting flow.
- Update-friendly: monthly or quarterly refreshes are straightforward.
- Auditable: assumptions, links, checks, versions, and source refreshes are easy to trace.
- Decision-useful: outputs highlight key variances and management takeaways, not just raw detail.

## Do No Harm

For existing workbooks:

- Do not break formulas, named ranges, tables, external links, query outputs, or sheet structure.
- Avoid inserting or deleting columns or moving report blocks unless explicitly requested.
- When adding rows, copy the nearest style, formula, and data-validation pattern.
- If placement is ambiguous, use the least-disruptive location and state the assumption.
- Prefer adding new analysis on a new sheet rather than heavily rewriting a live reporting or budget-collection workbook.
- Preserve existing version, refresh, source, scenario, and reporting-period conventions.

## Workbook Structure

For new FP&A workbooks, include only the tabs that are relevant:

- `Summary`: headline KPIs, key variances, scenario, version/date, and refresh timestamp.
- `Assumptions`: central drivers, policy assumptions, and scenario toggles.
- `Data_Links`: imports, mappings, source references, and refresh controls.
- `Operational_Drivers`: volume, price, mix, headcount, unit cost, and timing drivers.
- `Forecast_Model`: monthly or quarterly calculations.
- `Variance_Analysis`: actual vs budget, forecast, or prior-year views.
- `Dashboard`: executive-facing charts and summary tables.
- `Scenarios`: scenario tables, selector logic, and sensitivity outputs.
- `Checks`: sheet-level and workbook-level controls.
- `Sources`: source log, report names, extract dates, owners, and notes.

If editing an existing workbook, adapt to its current layout instead of forcing these tabs.

## FP&A Formatting Defaults

Use these defaults only when they do not conflict with user instructions or existing workbook conventions:

- Label units in headers: `Revenue ($000s)`, `GM (%)`, `Headcount (FTE)`, `AR Days`.
- Use real date values and consistent fiscal period labels.
- Display zeros as `-` using number formats, not text.
- Use one documented sign convention for negatives; if no house style exists, use red and parentheses for presentation.
- Use 0-1 decimals for currency depending on units, 1 decimal for percentages, and enough precision for operational metrics to be decision-useful without noise.
- Use whitespace and selective borders instead of boxing every cell.
- Hide gridlines on summary/dashboard tabs only when explicit styling carries the structure. Data and input tabs may retain gridlines for traceability.

## Periodicity And Time Axis

- Design at the most granular period needed, usually monthly for FP&A.
- Do not mix monthly, quarterly, and annual columns inside the same core time-series block.
- If quarter or year summaries are needed, aggregate them on separate summary blocks or sheets.
- Main model tabs should use the same start column and period order where possible.
- Clearly label Actual, Budget, Forecast, and Prior Year using one consistent convention.
- Visually separate actuals from forecasts with restrained shading or a clear boundary.
- When extending timelines, preserve copy-across formula consistency and formats.

## Inputs, Calculations, And Outputs

- Keep changeable assumptions separate from calculation blocks and clearly labeled.
- Hardcodes belong in assumptions/input areas, not buried inside calculations.
- Calculation blocks should read left-to-right with minimal jumps between tabs.
- Outputs should prioritize the management question: key KPIs, trends, variances, and the main drivers.
- If the workbook is used for distributed inputs, make input zones visually obvious and place instructions close to editable cells.
- Scenario toggles should be centralized and visible on summary/output tabs.

## Variance Analysis

Default comparison order:

`Actual | Budget/Forecast | Var $ | Var % | Prior Year`

Use only the columns relevant to the task.

- Make favorable/unfavorable treatment explicit; do not assume higher is always better.
- Variance columns should sit adjacent to the related comparison.
- Separate price, volume, mix, rate, and timing effects when management needs a driver bridge.
- Totals and sub-bridges must reconcile back to the reported variance.
- Use waterfall charts for bridge or variance explanations when helpful.

## Reporting And Dashboards

- Design outputs first: start from the management question and work backward to the required inputs and calculations.
- Executive-facing tabs should show key KPIs, trend, variance, and the 2-4 main drivers.
- Prefer line charts for trends, bar/column charts for comparisons, combo charts for value plus percent views, waterfall charts for variance bridges, and bullet-style visuals for actual vs target.
- Use sparklines or conditional formatting to surface trend and exceptions without overcrowding.
- Avoid 3D charts, excessive legends, too many series, heavy borders, and decorative fills.
- Chart titles must include the metric and unit, and labels must stay readable in PDF/print form when applicable.

## External Data And Refresh Discipline

- Centralize external links and imported data on a dedicated tab rather than scattering live links throughout the workbook.
- Clearly label source system, report name, as-of date, refresh date/time, and manual mapping steps.
- Avoid live external links unless necessary; if used, make them easy to find, test, and repair.
- Imported actuals should tie to reported totals used in management reporting.
- When manual paste/update is required, leave concise refresh instructions near the data block.

## Checks And Controls

Create a Checks sheet or checks block for nontrivial FP&A workbooks. At the top, include a model status summary:

- `MODEL STATUS: PASS/FAIL`, where PASS only means all required checks pass.
- Failed checks first, with `Check | Delta | Where to fix | Notes`.
- `None` if there are no failures.

Useful checks include:

- Actuals import ties to source totals.
- Monthly periods roll correctly into quarter/year summaries.
- Scenario selector points to a valid case.
- Headcount, opex, revenue, and cash roll-forwards reconcile.
- Alternate cuts reconcile, such as by function vs by account.
- Required assumptions are populated and within sensible bounds.
- No unexpected sign flips or broken variance formulas.

## Sources, Versions, And Metadata

Use a Sources table with columns like:

`Item | Value | Units | Period/As-of | Source Type | Source Name | Ref | Owner | Notes | Accessed/Refreshed (YYYY-MM-DD)`

For hardcoded input cells, add notes where feasible:

`Source: <system/report/document> | As-of: <YYYY-MM-DD> | Ref: <page/tab/field> | Notes: <short>`

If estimated:

`Assumption: <reason> | Owner: <role> | Date: <YYYY-MM-DD>`

Every recurring deliverable should show version/date/owner/scenario on the summary or cover tab. If assumptions change materially, include a version log or summary note.

## Definition Of Done

- Outputs clearly answer the management question.
- Actual, budget, forecast, and prior-year labels are consistent.
- Timeline periodicity is consistent and not mixed inside core blocks.
- Inputs are obvious; calculations do not hide hardcodes.
- External links/imports are centralized and documented.
- Key controls pass, or failures are clearly explained.
- Version/date/owner/scenario are visible where relevant.
- Summary/dashboard tabs are presentation-ready.
- Major inputs and manual adjustments are sourced or explicitly marked assumptions.
