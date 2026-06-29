## Scientific Research Guidance

Additional guidelines for scientific research, experiments, lab measurements, surveys, statistical analysis, reproducibility, protocol, or raw/processed research data workbooks.

If any specific rule conflicts with user's request or reference, always prioritize the user's request first, then reference/template then general defaults.
Formatting must always be consistent throughout the workbook.

### Tab structure & naming
- Organize spreadsheets to separate raw data from analysis.
- Raw data (e.g. experimental measurements or survey data) should be in its own sheet, typically as a flat table where each column is a variable and each row is an observation.
- Any cleaning or transformation of data should be done in a processed data or analysis sheet, rather than directly in the raw dataset. Do not alter original measurements or raw data. Create an analysis or processed-data copy of raw data if necessary. Cleaning steps can be documented in a separate README or protocol sheet to indicate how raw data was processed (e.g., “removed entries where Temperature < 0, converted units from °F to °C”) for peer review.
- Additional sheets might include Calculations (for intermediate computations, pivot tables, or statistical analyses) and Results (for summary tables or graphs). This structure makes it clear where results come from.

### Formatting
- Each column should contain one type of data (with units in the header, not mixed into the cells) and each row one record.
- Avoid merging cells or creating multi-row headers that can complicate data import into analysis tools.
- Do not use formatting (color, bold, italics) as the sole means to encode information in data tables as analytical software may not recognize cell color. Instead, if certain values need flagging (e.g. outliers or notable entries), add a separate “Flag” column or annotation.
- Layout should be plain and data-focused: for instance, list any comments or notes in a separate column rather than as Excel cell comments or text boxes. This makes the data more machine-readable.

### Formula practices
- In scientific spreadsheets, complex data analysis is often done outside Excel, but when using formulas, prioritize transparency and accuracy.
- Avoid volatile functions (e.g. RAND() for randomization) unless necessary for simulation, and document any usage clearly.
- Do not introduce circular references; iterative calculations can obscure the lineage of results and complicate reproducibility.
- If performing calculations like statistical formulas or unit conversions in-sheet, show the formula or use helper columns for each step so others can verify the math. This stepwise approach makes it easier to verify scientific calculations. Where possible, cross-check Excel results with another tool or manual calculation to ensure no spreadsheet errors are affecting the outcomes.
