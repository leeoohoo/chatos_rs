## Healthcare (clinical/administrative) spreadsheets

Additional guidelines for healthcare, clinical, patient, medical, hospital, staffing, care delivery, or healthcare administration workbooks.

If any specific rule conflicts with user's request or reference, always prioritize the user's request first, then reference/template then general defaults.
Formatting must always be consistent throughout the workbook.

### Tab structure
- Use separate tabs for distinct data sets and separate raw data from summary or analysis: Data Entry (raw clinical or administrative data inputs), Calculations (where any metrics or aggregations are computed), and Reports (for summaries, charts, or tables for decision-making).
- In a clinical context, you might have one sheet for patient-level data (each row = one patient or encounter), and another sheet that pivots or summarizes by clinic or by month.
- Administrative workbooks (like a staffing or finance tracker) might split sheets by function – e.g., a “Staff List” sheet, a “Schedule” sheet, a “Payroll Calc” sheet, etc.
- Name each tab descriptively, such as “Appointments_RawData”, “Clinic KPI Dashboard”.

### Formatting
- Clarity and readability can have real consequences in healthcare, so adhere to common formatting signals.
- Highlight input fields (shaded light yellow) or form elements that staff are meant to fill out. If input is not used for calculation, placeholder “enter data here” may be used.
- Use conditional formatting to flag values that are out of normal range or require attention: e.g., automatically shade a cell red if a lab value is critical, or highlight an entire row if a patient is overdue for a follow-up.
- Include color legend (e.g. red = critical, orange = caution, green = within target, etc.).
- Use clear data formatting (e.g. YYYY-MM-DD or MM/DD/YYYY as required), identifiers (like medical record numbers), and units for all measurements (e.g. weight in kg, temperature in °C).
- Layout must be printable and scannable in urgent situations: use adequate font sizes, avoid crowding too much info, and freeze header rows for long lists of data so column titles (like “Patient ID”, “Last Visit Date”) are always visible.


### Formulas
- Accuracy is paramount, so double-check and simplify formulas wherever possible. If calculating clinical indicators (e.g. average length of stay, readmission rates), use straightforward aggregate formulas or pivot tables, which are less error-prone than manually crafted arrays.
- Where calculations involve thresholds or guidelines (say, flagging patients with BMI over a certain value), consider referencing those thresholds from a single cell named “BMI_threshold” so that it’s clear and adjustable, rather than embedding the number in multiple formulas.
- Include sanity checks: for example, if you calculate a total patient count from different categories, include a check that the sum of categories equals the total from the raw data. Any critical formula (like one computing a dosage or a financial total) might be independently verified or at least labeled clearly with comments.

### Raw data and outputs
- Never alter or delete original healthcare data
- For data entries that need correction (e.g., a misspelled diagnosis or an out-of-range age), consider doing it via a data validation process or noting corrections in a separate column (“Corrected value”) rather than overwriting, or maintain an audit log of changes.


### Metadata, units, and codes
- Healthcare data is full of codes and units, which must be documented.
- Always provide the units for each metric (e.g., blood glucose “mg/dL”, heart rate “bpm”).
- If the sheet is used for clinical purposes, including normal ranges or target ranges in the metadata can be helpful (for instance, note “Normal range: 70–100 mg/dL” next to a blood sugar metric).
- Make use of separate sheets to list code definitions: for example, a sheet “Codebook” could map ICD-10 codes or lab test codes to plain English descriptions.
- If the main data uses abbreviations (like “M”/“F” for sex or clinic codes like “NYC” for New York Clinic), ensure there is a legend or the full term in the header (“Sex (M/F)” is clear).
