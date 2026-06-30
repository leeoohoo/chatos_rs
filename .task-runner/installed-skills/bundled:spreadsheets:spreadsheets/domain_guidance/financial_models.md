# Financial Modeling Guidance

## Scope And Priority

Use this guidance for:

- 3-statement models, DCFs, LBOs, debt schedules, operating models, scenario models, sensitivity tables, comps, valuation bridges, and finance dashboards.
- Requests to make a workbook analyst-style, investment-banking quality, board-ready, IC-ready, lender-ready, or institutional finance quality.
- Updating assumptions, extending timelines, adding scenarios, adding checks, or improving source documentation in a finance workbook.

Do not use this guidance for generic non-finance trackers, planners, rosters, lightweight survey summaries, or project logs unless the user explicitly wants finance-model-style treatment. For budgets, rolling forecasts, monthly business reviews, KPI packs, headcount plans, opex planning, revenue planning, variance analysis, or FP&A operating plans, also read `domain_guidance/corporate_finance_fpa.md`.

If any specific rule conflicts with user's request or reference, always prioritize the user's request first, then reference/template then general finance defaults.
Formatting must always be consistent throughout the workbook.

## Reference following: Company-specific model translation

When adapting a reference financial model to a different company, preserve its analytical depth — not just its layout and formatting. Map each material reference driver or schedule to the closest target-company equivalent using reported KPIs, guidance, and primary-source disclosures.

Do not replace a bottom-up reference model with generic revenue-growth and margin assumptions when company-specific drivers are available. Any material schedule that is omitted or simplified must be explicitly justified and disclosed. Follow formatting rules below unless they directly differ from the reference. The reference formatting always takes priority.

## Row And Cell Classes

Classify target areas into these types before styling:

- `title`: model or report names, usually sparse top rows with prominent text.
- `metadata`: version, date, author, currency, units, scenario, source notes, or as-of context.
- `period_header`: month, quarter, fiscal year, calendar year, actual, estimate, forecast, scenario, or period labels.
- `section_header`: labels introducing major blocks.
- `subheader`: labels within a section, often bold, lightly filled, underlined, or bordered.
- `body`: normal calculation, source, or data rows.
- `total`: total, subtotal, EBITDA, EBIT, FCF, ending balance, net debt, valuation, checksum, or key output rows.
- `input`: editable assumptions or user-controlled drivers.
- `linked_imported`: formulas or values linked/imported from other sheets, files, systems, filings, source tabs, or third-party datasets.
- `check`: pass/fail, delta, tie-out, balance check, error flag, warning, or sanity-check rows.
- `spacer_note`: blank separators, footnotes, source notes, explanatory text, caveats, and review comments.

Do not classify one-cell section labels as period headers. Do not classify model titles or metadata rows as table headers.

## New creations and defaults

### Formatting Rules
Follow these default financial formatting industry conventions unless specifically overridden:

#### Color Formatting Conventions
- **Blue text (RGB: 0,0,255)**: Editable assumptions, user-controlled inputs, and scenario drivers
- **Black text (RGB: 0,0,0)**: ALL formulas and calculations
- **Green text (RGB: 0,128,0)**: Links/References to other worksheets within same workbook
- **Red text (RGB: 255,0,0)**: External links to other files
- **Yellow background (RGB: 255,255,0)**: Key assumptions needing attention or cells that need to be updated
- Include a compact legend if the workbook has more than one type of input, formula, or link coloring; legend colors must match the actual styles.
- Do not color historical actuals, imported source data, constants from filings, labels, dates, IDs, metadata, or source-tab values blue unless the user should edit them.

#### Number Formatting
- All number and date formats must be set (to both source values and formula outputs), with numbers right-aligned.
- Use real date values for period headers and format forecast periods like `yyyy"E"` instead of typing `2027E`.
- Default finance formats:
  ```
  Currency/financial amounts: "$#,##0;[Red]($#,##0);-"
  Per-share values: "$0.00;[Red]($0.00);-"
  Percentages: "0.0%;[Red](0.0%);-"
  Multiples: "0.0x;[Red](0.0x);-"
  Counts/non-currency amounts: "#,##0;[Red](#,##0);-"
  ```

### Workbook Layout and Structure
When creating a finance model from scratch, use a readable flow such as cover/summary, assumptions, drivers, financials/model, valuation or outputs, sensitivities/scenarios, checks, and Sources/Audit. Adapt this structure to the request and preserve existing workbook architecture when editing a live model. These are good rules to follow:

- Inputs, calculations and outputs should be organized into separate sheets
- All added raw inputs should have their sources cited in the appropriate cell comment
- ALL assumptions (growth rates, multiples, margins, tax rates, WACC, terminal growth, etc.) should be in separate assumption cells or sheets.
- Anchor forecast assumptions to history, source data, or a visible ramp/step change where possible.
- Do not embed business assumptions or source data as magic numbers inside formulas. Put them in labeled input/source cells and reference them.
- State core model conventions in the workbook: currency/unit scale, fiscal period basis, forecast period, valuation date, source date, scenario/case, and discounting convention where relevant.
- Label units in headers or row labels (for example, `Revenue ($mm)`, `Margin %`, `EV/EBITDA` `Gross Income ($mm)`).
- Wrap text for long notes or descriptions.
- Source, historical-data, notes, and description columns must be readable at normal zoom. Use compact source IDs in historical/model rows and put full URLs in a separate Sources/Audit sheet; do not repeat long URLs down working tables. Sources/Audit should capture item, value, units, period/as-of date, source name/link, and notes. Use aliases or a mapping table for long XBRL tags. Widen text columns and use wrap text/row height so filing references, source notes, and audit notes are not clipped.
- Complex calculations or assumptions can be explained via a cell comment.
- If many iterations are requested, maintain a version history or changelog (often on the cover sheet or in a separate tab) to track updates.
- For complex workbooks with many tabs, have a `Cover` tab that indicates helpful information about the workbook, e.g. the model purpose, version/date, key outputs, overview, and instructions for use.
- Tables, charts and graphs can be used to summarize important information.
- Build in error checks such as ensuring balance sheet balances when possible.

#### For 3-statement or IB-style models
- Use explicit forecast drivers instead of hardcoding outputs.
- Retained earnings should roll forward from beginning balance, net income, distributions, repurchases, and other equity movements.
- Cash flow statement ending cash should tie to balance sheet cash.
- Debt, cash, and share count schedules should be explicit when they affect valuation.
- Do not let balance-sheet checks pass only because cash, equity, or "other" rows are plugged. If a plug is unavoidable, label and justify it.

#### On Timeline and actuals vs forecast
For period-based models:
- Use one consistent time axis per block; clearly label actuals, budget/forecast, prior year, forecast periods, and the period basis.
- Visually separate historical/actual from forecast periods, and keep copy-across formulas consistent across each time series.
- Do not mix monthly, quarterly, and annual periods in the same calculation block unless separated and tied with clear rollups.


### Formula Rules
In additional to the formula rules already specified in SKILL.md, follow these rules:
- No harcoded numbers inside calculation areas unless explicitly allowed. Always ensure color formatting conventions are properly applied.
- Legible formulas that is **easily auditable** and follow industry guidelines for the task and end user is important. Instead of dense nested formulas for complex logic, prefer helper rows and Excel formulas to simplify. Users should be able to trace the model from inputs to outputs.
- Use standard Excel functions and common financial functions when they improve auditability: NPV/XNPV, IRR/XIRR, PMT/IPMT, SLN/DB/DDB, and exact-match lookups such as INDEX/MATCH or XLOOKUP where appropriate. Keep formulas readable and source assumptions from input cells.
- Calculations must be formula driven, and a single formula pattern across forecast periods where possible.
- Avoid volatile functions (**INDIRECT, OFFSET**) unless required.
- Time series logic should be **copy-across consistent** (same formula pattern across periods).
- Use SUM across the range above for totals; avoid “sum of parts” that skips lines.
- Avoid external workbook links unless explicitly requested, or it already exists; if unavoidable, label and color-code them red.
- Avoid circular references in most cases, as they can make models unstable and difficult to audit. However, in certain financial models: such as cash flow sweeps, interest on average balances, or working capital loops, they may be required. When circular logic is intentional, clearly document the purpose and ensure that iteration settings are configured correctly. Surface it on `Checks` and avoid hacks.
- Financial return formulas must be guarded until the cash-flow sign pattern and minimum data requirements are valid. If IRR/XIRR cannot be valid for an illustrative template, use a documented estimate metric such as cash-on-cash return, NPV at the stated discount rate, or a guarded RATE approximation instead of surfacing `#NUM!`.

### Sensitivity/scenario table correctness
- Changed drivers should be obvious in row/column headers.
- Each output cell must calculate from the row/column driver inputs and the target output cell or equivalent driver logic. Do not paste static sensitivity outputs.
- Sensitivities must recalculate the underlying valuation or return mechanics, not just tweak final outputs.
- Use helper rows/blocks for PV of FCF, terminal value, equity bridge, per-share value, returns, or other intermediate outputs instead of hiding huge formulas in the table body.

### DCF and valuation minimums
If the request involves DCF, company valuation, investment banking, equity research, or similar:
- Build within the finance model architecture above; for DCF/valuation, make the valuation output, sensitivities/scenarios, checks, and Sources/Audit explicit.
- Include the key DCF bridge unless the prompt specifies otherwise: revenue/EBIT or EBITDA drivers, taxes, D&A, capex, change in NWC, unlevered FCF, discount factors, PV of forecast FCF, terminal value, PV of terminal value, enterprise value, and equity value bridge when net debt/share data is available.
- Label simplified assumptions explicitly if source data is missing. Do not imply precision where the prompt or inputs do not support it.
- Document whether terminal value uses Gordon growth or exit multiple, and ensure the terminal value is discounted using the same timing convention as forecast cash flows.
- Follow the sensitivity/scenario table correctness rules above.

### Investment Banking Guidance
If the spreadsheet is related to investment banking (LBO, DCF, 3-statement, valuation model, or similar) and a reference/template workbook is not provided:
- Total calculations should sum a range of cells directly above them.
- Hide gridlines. Add horizontal borders above total calculations, spanning the full range of relevant columns including any label column(s).
- Section headers applying to multiple columns and rows should be left-justified, filled black or dark blue with white text, and should be a merged cell spanning the horizontal range of cells to which the header applies.
- Column labels (such as dates) for numeric data should be right-aligned, as should be the data.
- Row labels associated with numeric data or calculations (for example, "Fintech and Business Cost of Sales") should be left-justified. Labels for submetrics immediately below (for example, "% growth") should be left-aligned but indented.
- Freeze panes on large model sheets and avoid hiding rows/columns; use grouping sparingly when needed.


## Editing or formatting existing workbooks
- For formatting-only or restyling edits, preserve formulas, named ranges, tables, external links, hidden rows/columns, sheet order, and semantically correct existing formats unless the user asks to restructure. Prefer new analysis sheets when that avoids disrupting a live model.
- Preserve workbook navigation and period semantics: freeze panes, filters, grouped headers, and date/period header formats should remain intact unless explicitly changed.
- Classify row formats from labels, existing formats, nearby context, and sample values before applying ranges. Do not convert source values to percentages unless the row is clearly a margin, rate, growth, yield, WACC/TGR, cost of equity/debt, discount rate, risk-free rate, risk premium, or tax-rate row.

## Required Audit Pass
- Correctness is reputation-critical. For source-backed finance work, linked models, valuation, forecasting, and other high-impact analysis, run an additional finance audit pass and fix meaningful issues; do not rely only on a generic formula-error scan.

For complex financial models, DCFs, 3-statement models, scenario/sensitivity models, or source-backed finance analysis, before final export:
- Confirm sources, assumptions, and representative formulas tie together; trace representative cells when it clarifies a high-impact calculation or check.
- Check that income statement, balance sheet, cash flow, DCF/valuation, sensitivity/scenario tables, and checks tie together where present.
- Review large forecast step-changes versus history; fix them, bridge them with driver logic, or add a clear source/assumption note.
- Confirm any cash/equity/other plug is clearly labeled and justified; do not treat a passing balance-sheet check as sufficient validation.
- Use helper rows or blocks for complex formulas instead of long opaque formulas in output tables.

### `Checks` and Additional Verification Guidance
- Create or maintain a visible `Checks` section or sheet when the model is nontrivial. At minimum, include checks for formula errors, source/input completeness, totals vs components, sign/units, and model status.
- Financial checks should be decomposed into readable rows: one assertion per row with labeled Actual, Expected, Difference, Tolerance, Status, and Notes columns. The final model-status formula should aggregate check statuses with conditional formatting (e.g. with "OK" being green fill), not recompute business logic inline.
- For larger finance workbooks, surface a model status on the cover/summary; failed checks should show a fix hint or location.
- For IB-style, 3-statement, or operating models, include applicable balance sheet balance, cash flow/cash roll-forward, debt roll-forward, sign convention, total/subtotal tie-out, and revenue/margin sanity checks.
- For valuation models, include checks that free cash flow ties to its components, discount factors and terminal value are correct, enterprise value bridges to equity value when relevant, and key valuation outputs are not hardcoded.
- After bulk formatting, spot-check representative row labels against formats so rate/percentage rows and period/date headers were not converted to currency or plain numbers.
- Before final export:
   1. inspect representative formulas/styles and verify color conventions
   2. checks show `OK` or clearly explain any limitation.
   3. all sheets are rendered and content is visible - fix any clipped labels, source notes, formulas, or important outputs before returning the workbook.
   4. additional audit pass was successful and high-priority issues were fixed.
