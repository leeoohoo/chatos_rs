// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const MACOS_RANGE_READ_SCRIPT: &str = r#"
(function () {
  ObjC.import("Foundation");
  ObjC.import("AppKit");

  const inputData = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
  const inputText = ObjC.unwrap($.NSString.alloc.initWithDataEncoding(inputData, $.NSUTF8StringEncoding));
  const request = JSON.parse(String(inputText));
  const excel = Application("Microsoft Excel");
  if (!excel.running()) throw new Error("Microsoft Excel is not running");

  function runtimeInstance() {
    const running = $.NSRunningApplication.runningApplicationsWithBundleIdentifier("com.microsoft.Excel");
    if (running.count < 1) throw new Error("Excel process identity is unavailable");
    return String(running.objectAtIndex(0).processIdentifier);
  }

  function boundedText(raw) {
    const clean = Array.from(String(raw).replace(/[\u0000-\u001f\u007f]/g, "\ufffd"));
    return {
      value: clean.slice(0, 128).join(""),
      truncated: clean.length > 128
    };
  }

  function boundedIdentityText(raw) {
    const source = String(raw);
    const lossy = /[\u0000-\u001f\u007f]/.test(source);
    const clean = Array.from(source.replace(/[\u0000-\u001f\u007f]/g, "\ufffd"));
    return {
      value: clean.slice(0, 128).join(""),
      truncated: lossy || clean.length > 128
    };
  }

  function externalFormula(formula) {
    return /\[[^\]]+\][^!]*!/i.test(formula) ||
      /(?:https?|file):\/\//i.test(formula) ||
      /\\\\/.test(formula) ||
      /[A-Za-z]:\\/.test(formula);
  }

  function safeScalar(raw) {
    if (raw === null || raw === undefined) return { value: null, truncated: false };
    if (typeof raw === "boolean") return { value: raw, truncated: false };
    if (typeof raw === "number" && Number.isFinite(raw)) return { value: raw, truncated: false };
    if (typeof raw === "string") return boundedText(raw);
    return { value: null, truncated: false };
  }

  function selectAndVerify() {
    if (runtimeInstance() !== request.runtime_instance) throw new Error("Excel process identity changed");
    const workbooks = excel.workbooks();
    if (request.workbook_index < 1 || request.workbook_index > workbooks.length) {
      throw new Error("Excel workbook position is stale");
    }
    const workbook = workbooks[request.workbook_index - 1];
    const workbookName = String(workbook.name());
    let workbookFullName = workbookName;
    try { workbookFullName = String(workbook.fullName()); } catch (_) {}
    if (workbookName !== request.workbook_name || workbookFullName !== request.workbook_identity_source) {
      throw new Error("Excel workbook identity is stale");
    }
    const worksheets = workbook.worksheets();
    if (request.worksheet_index < 1 || request.worksheet_index > worksheets.length) {
      throw new Error("Excel worksheet position is stale");
    }
    const worksheet = worksheets[request.worksheet_index - 1];
    if (String(worksheet.name()) !== request.worksheet_name) {
      throw new Error("Excel worksheet identity is stale");
    }
    return { workbook: workbook, worksheet: worksheet };
  }

  const selected = selectAndVerify();
  const targetRange = selected.worksheet.ranges.byName(request.range_address);
  if (Number(targetRange.firstRowIndex()) !== request.start_row ||
      Number(targetRange.firstColumnIndex()) !== request.start_column ||
      targetRange.rows().length !== request.row_count ||
      targetRange.columns().length !== request.column_count) {
    throw new Error("Excel returned a non-exact range");
  }
  const sourceCells = targetRange.cells();
  if (sourceCells.length !== request.cell_count) throw new Error("Excel returned an unexpected cell count");

  const cells = [];
  for (let index = 0; index < sourceCells.length; index += 1) {
    const cell = sourceCells[index];
    /*__CHATOS_MACOS_CELL_STATE__*/
    cells.push({
      row_offset: Math.floor(index / request.column_count),
      column_offset: index % request.column_count,
      value: value.value,
      value_truncated: value.truncated,
      displayed_text: displayed.value,
      displayed_text_truncated: displayed.truncated,
      has_formula: hasFormula,
      formula: formula,
      formula_truncated: formulaTruncated,
      formula_hidden: formulaHidden,
      formula_external_reference: formulaExternalReference,
      number_format: numberFormat.value,
      number_format_truncated: numberFormat.truncated,
      number_format_unavailable: numberFormatUnavailable,
      is_error: isError
    });
  }

  selectAndVerify();
  return JSON.stringify({
    schema_version: 1,
    runtime_instance: request.runtime_instance,
    workbook_index: request.workbook_index,
    workbook_name: request.workbook_name,
    worksheet_index: request.worksheet_index,
    worksheet_name: request.worksheet_name,
    range_address: request.range_address,
    start_row: request.start_row,
    start_column: request.start_column,
    row_count: request.row_count,
    column_count: request.column_count,
    cell_count: request.cell_count,
    cells: cells
  });
})()
"#;

const WINDOWS_RANGE_READ_SCRIPT: &str = r#"
# __CHATOS_WINDOWS_PREAMBLE__
function Select-AndVerify($excel, $request) {
  if ((Get-ExcelRuntimeInstance $excel) -ne [string]$request.runtime_instance) {
    throw 'Excel process identity changed'
  }
  $workbookIndex = [int]$request.workbook_index
  if ($workbookIndex -lt 1 -or $workbookIndex -gt [int]$excel.Workbooks.Count) {
    throw 'Excel workbook position is stale'
  }
  $workbook = $excel.Workbooks.Item($workbookIndex)
  $workbookName = [string]$workbook.Name
  try { $workbookFullName = [string]$workbook.FullName } catch { $workbookFullName = $workbookName }
  if ($workbookName -cne [string]$request.workbook_name -or
      $workbookFullName -cne [string]$request.workbook_identity_source) {
    throw 'Excel workbook identity is stale'
  }
  $worksheetIndex = [int]$request.worksheet_index
  if ($worksheetIndex -lt 1 -or $worksheetIndex -gt [int]$workbook.Worksheets.Count) {
    throw 'Excel worksheet position is stale'
  }
  $worksheet = $workbook.Worksheets.Item($worksheetIndex)
  if ([string]$worksheet.Name -cne [string]$request.worksheet_name) {
    throw 'Excel worksheet identity is stale'
  }
  return [ordered]@{ workbook = $workbook; worksheet = $worksheet }
}

$excelType = [Type]::GetTypeFromProgID('Excel.Application', $false)
if ($null -eq $excelType) { throw 'Microsoft Excel is not installed' }
$excel = [Runtime.InteropServices.Marshal]::GetActiveObject('Excel.Application')
$selected = Select-AndVerify $excel $request
$range = $selected.worksheet.Range([string]$request.range_address)
if ([int]$range.Row -ne [int]$request.start_row -or
    [int]$range.Column -ne [int]$request.start_column -or
    [int]$range.Rows.Count -ne [int]$request.row_count -or
    [int]$range.Columns.Count -ne [int]$request.column_count -or
    [int]$range.Cells.Count -ne [int]$request.cell_count) {
  throw 'Excel returned a non-exact range'
}

$cells = @()
for ($row = 1; $row -le [int]$request.row_count; $row += 1) {
  for ($column = 1; $column -le [int]$request.column_count; $column += 1) {
    $cell = $range.Cells.Item($row, $column)
    # __CHATOS_WINDOWS_CELL_STATE__
    $cells += [ordered]@{
      row_offset = $row - 1
      column_offset = $column - 1
      value = $value
      value_truncated = $valueTruncated
      displayed_text = $displayed.value
      displayed_text_truncated = $displayed.truncated
      has_formula = $hasFormula
      formula = $formula
      formula_truncated = $formulaTruncated
      formula_hidden = $formulaHidden
      formula_external_reference = $formulaExternalReference
      number_format = $numberFormat.value
      number_format_truncated = $numberFormat.truncated
      number_format_unavailable = $numberFormatUnavailable
      is_error = $isError
    }
  }
}

$null = Select-AndVerify $excel $request
$result = [ordered]@{
  schema_version = 1
  runtime_instance = [string]$request.runtime_instance
  workbook_index = [int]$request.workbook_index
  workbook_name = [string]$request.workbook_name
  worksheet_index = [int]$request.worksheet_index
  worksheet_name = [string]$request.worksheet_name
  range_address = [string]$request.range_address
  start_row = [int]$request.start_row
  start_column = [int]$request.start_column
  row_count = [int]$request.row_count
  column_count = [int]$request.column_count
  cell_count = [int]$request.cell_count
  cells = $cells
}
$result | ConvertTo-Json -Depth 8 -Compress
"#;

pub(super) fn macos_range_read_script() -> String {
    super::script_fragments::expand_script(MACOS_RANGE_READ_SCRIPT)
}

pub(super) fn windows_range_read_script() -> String {
    super::script_fragments::expand_script(WINDOWS_RANGE_READ_SCRIPT)
}
