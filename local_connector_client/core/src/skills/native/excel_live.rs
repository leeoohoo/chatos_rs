// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const MAX_BRIDGE_OUTPUT_BYTES: u64 = 512 * 1024;
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(8);
const WRITE_BRIDGE_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_OPEN_WORKBOOKS: usize = 32;
const MAX_WORKSHEETS_PER_WORKBOOK: usize = 64;
const MAX_WORKBOOK_NAME_CHARACTERS: usize = 512;
const MAX_WORKSHEET_NAME_CHARACTERS: usize = 64;
const MAX_IDENTITY_SOURCE_CHARACTERS: usize = 4096;
const MAX_RANGE_CELLS: usize = 256;
const MAX_CELL_TEXT_CHARACTERS: usize = 128;
const MAX_NUMBER_FORMAT_CHARACTERS: usize = 128;
const MAX_SNAPSHOT_ID_CHARACTERS: usize = 96;
const MAX_EXCEL_ROWS: usize = 1_048_576;
const MAX_EXCEL_COLUMNS: usize = 16_384;

const MACOS_OSASCRIPT_PATH: &str = "/usr/bin/osascript";
const MACOS_EXCEL_APPLICATION_PATH: &str = "/Applications/Microsoft Excel.app";
static EXCEL_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Eq, PartialEq)]
struct A1Range {
    canonical: String,
    start_row: usize,
    start_column: usize,
    end_row: usize,
    end_column: usize,
    row_count: usize,
    column_count: usize,
    cell_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RangeReadTarget {
    runtime_instance: String,
    workbook_id: String,
    workbook_index: usize,
    workbook_name: String,
    workbook_identity_source: String,
    workbook_read_only: bool,
    worksheet_id: String,
    worksheet_index: usize,
    worksheet_name: String,
    worksheet_visibility: String,
    worksheet_protected: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum WriteCell {
    Blank,
    Value(Value),
    Formula(String),
}

#[derive(Clone, Debug, PartialEq)]
struct RangeWriteInput {
    workbook_id: String,
    worksheet_id: String,
    range: A1Range,
    expected_snapshot_id: String,
    cells: Vec<WriteCell>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RangeFormatInput {
    workbook_id: String,
    worksheet_id: String,
    range: A1Range,
    expected_snapshot_id: String,
    preset: String,
    number_format: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WriteCellSummary {
    blank_cells: usize,
    value_cells: usize,
    formula_cells: usize,
    text_characters: usize,
    content_sha256: String,
}

const MACOS_STATUS_SCRIPT: &str = r#"
(function () {
  const result = {
    schema_version: 1,
    installed: true,
    running: false,
    runtime_instance: null,
    application_version: null,
    workbooks_total: 0,
    workbooks_truncated: false,
    workbook_metadata_omitted: true,
    workbooks: []
  };
  const excel = Application("Microsoft Excel");
  if (!excel.running()) {
    return JSON.stringify(result);
  }
  result.running = true;
  try {
    ObjC.import("AppKit");
    const running = $.NSRunningApplication.runningApplicationsWithBundleIdentifier("com.microsoft.Excel");
    if (running.count > 0) {
      result.runtime_instance = String(running.objectAtIndex(0).processIdentifier);
    }
  } catch (_) {}
  if (result.runtime_instance === null) {
    try {
      const current = Application.currentApplication();
      current.includeStandardAdditions = true;
      result.runtime_instance = String(current.doShellScript("/usr/bin/pgrep -x 'Microsoft Excel' | /usr/bin/head -n 1"));
    } catch (_) {}
  }
  try { result.application_version = String(excel.version()); } catch (_) {}
  try {
    result.workbooks_total = excel.workbooks().length;
    result.workbooks_truncated = result.workbooks_total > 32;
  } catch (_) {}
  return JSON.stringify(result);
})()
"#;

const MACOS_SNAPSHOT_SCRIPT: &str = r#"
(function () {
  const result = {
    schema_version: 1,
    installed: true,
    running: false,
    runtime_instance: null,
    application_version: null,
    workbooks_total: 0,
    workbooks_truncated: false,
    workbooks: []
  };
  const excel = Application("Microsoft Excel");
  if (!excel.running()) {
    return JSON.stringify(result);
  }
  result.running = true;
  try {
    ObjC.import("AppKit");
    const running = $.NSRunningApplication.runningApplicationsWithBundleIdentifier("com.microsoft.Excel");
    if (running.count > 0) {
      result.runtime_instance = String(running.objectAtIndex(0).processIdentifier);
    }
  } catch (_) {}
  if (result.runtime_instance === null) {
    try {
      const current = Application.currentApplication();
      current.includeStandardAdditions = true;
      result.runtime_instance = String(current.doShellScript("/usr/bin/pgrep -x 'Microsoft Excel' | /usr/bin/head -n 1"));
    } catch (_) {}
  }
  try { result.application_version = String(excel.version()); } catch (_) {}

  let workbooks = [];
  try { workbooks = excel.workbooks(); } catch (_) {}
  result.workbooks_total = workbooks.length;
  result.workbooks_truncated = workbooks.length > 32;

  let activeWorkbookName = null;
  let activeWorkbookFullName = null;
  try {
    const activeWorkbook = excel.activeWorkbook();
    activeWorkbookName = String(activeWorkbook.name());
    try { activeWorkbookFullName = String(activeWorkbook.fullName()); } catch (_) {}
  } catch (_) {}

  function sheetVisibility(raw) {
    const number = Number(raw);
    if (number === -1) return "visible";
    if (number === 0) return "hidden";
    if (number === 2) return "very_hidden";
    const text = String(raw).toLowerCase();
    if (text.indexOf("very") >= 0 && text.indexOf("hidden") >= 0) return "very_hidden";
    if (text.indexOf("hidden") >= 0) return "hidden";
    if (text.indexOf("visible") >= 0) return "visible";
    return "unknown";
  }

  const limit = Math.min(workbooks.length, 32);
  for (let index = 0; index < limit; index += 1) {
    const workbook = workbooks[index];
    let name = "";
    let fullName = "";
    let saved = false;
    let readOnly = false;
    try { name = String(workbook.name()); } catch (_) {}
    try { fullName = String(workbook.fullName()); } catch (_) { fullName = name; }
    try { saved = Boolean(workbook.saved()); } catch (_) {}
    try { readOnly = Boolean(workbook.readOnly()); } catch (_) {}
    const active = name === activeWorkbookName && fullName === (activeWorkbookFullName || fullName);

    let worksheets = [];
    try { worksheets = workbook.worksheets(); } catch (_) {}
    let activeSheetName = null;
    try { activeSheetName = String(workbook.activeSheet().name()); } catch (_) {}
    const sheetLimit = Math.min(worksheets.length, 64);
    const sheets = [];
    for (let sheetIndex = 0; sheetIndex < sheetLimit; sheetIndex += 1) {
      const sheet = worksheets[sheetIndex];
      let sheetName = "";
      let visible = "unknown";
      let protectedContents = false;
      try { sheetName = String(sheet.name()); } catch (_) {}
      try { visible = sheetVisibility(sheet.visible()); } catch (_) {}
      try { protectedContents = Boolean(sheet.protectContents()); } catch (_) {}
      sheets.push({
        index: sheetIndex + 1,
        name: sheetName,
        visible: visible,
        protected: protectedContents,
        active: active && sheetName === activeSheetName
      });
    }
    result.workbooks.push({
      index: index + 1,
      name: name,
      identity_source: fullName || name,
      saved: saved,
      read_only: readOnly,
      active: active,
      sheet_count: worksheets.length,
      sheets_truncated: worksheets.length > 64,
      sheets: sheets
    });
  }
  return JSON.stringify(result);
})()
"#;

const WINDOWS_SNAPSHOT_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$result = [ordered]@{
  schema_version = 1
  installed = $false
  running = $false
  runtime_instance = $null
  application_version = $null
  workbooks_total = 0
  workbooks_truncated = $false
  workbooks = @()
}
$excelType = [Type]::GetTypeFromProgID('Excel.Application', $false)
if ($null -eq $excelType) {
  $result | ConvertTo-Json -Depth 8 -Compress
  exit 0
}
$result.installed = $true
try {
  $excel = [Runtime.InteropServices.Marshal]::GetActiveObject('Excel.Application')
} catch {
  $result | ConvertTo-Json -Depth 8 -Compress
  exit 0
}
$result.running = $true
try { $result.application_version = [string]$excel.Version } catch {}
try {
  $result.runtime_instance = 'hwnd:' + [string]$excel.Hwnd
  $excelProcess = Get-Process -Name EXCEL -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -eq $excel.Hwnd } |
    Select-Object -First 1
  if ($null -ne $excelProcess) { $result.runtime_instance = [string]$excelProcess.Id }
} catch {}

$activeWorkbookName = $null
$activeWorkbookFullName = $null
try {
  $activeWorkbookName = [string]$excel.ActiveWorkbook.Name
  $activeWorkbookFullName = [string]$excel.ActiveWorkbook.FullName
} catch {}

$workbookCount = [int]$excel.Workbooks.Count
$result.workbooks_total = $workbookCount
$result.workbooks_truncated = $workbookCount -gt 32
$workbookLimit = [Math]::Min($workbookCount, 32)
$workbooks = @()
for ($index = 1; $index -le $workbookLimit; $index += 1) {
  $workbook = $excel.Workbooks.Item($index)
  $name = [string]$workbook.Name
  try { $fullName = [string]$workbook.FullName } catch { $fullName = $name }
  $active = $name -eq $activeWorkbookName -and $fullName -eq $activeWorkbookFullName
  $worksheetCount = [int]$workbook.Worksheets.Count
  $worksheetLimit = [Math]::Min($worksheetCount, 64)
  $activeSheetName = $null
  try { $activeSheetName = [string]$workbook.ActiveSheet.Name } catch {}
  $worksheets = @()
  for ($sheetIndex = 1; $sheetIndex -le $worksheetLimit; $sheetIndex += 1) {
    $sheet = $workbook.Worksheets.Item($sheetIndex)
    $visibility = switch ([int]$sheet.Visible) {
      -1 { 'visible' }
      0 { 'hidden' }
      2 { 'very_hidden' }
      default { 'unknown' }
    }
    $worksheets += [ordered]@{
      index = $sheetIndex
      name = [string]$sheet.Name
      visible = $visibility
      protected = [bool]$sheet.ProtectContents
      active = [bool]($active -and ([string]$sheet.Name -eq $activeSheetName))
    }
  }
  $workbooks += [ordered]@{
    index = $index
    name = $name
    identity_source = $(if ([string]::IsNullOrWhiteSpace($fullName)) { $name } else { $fullName })
    saved = [bool]$workbook.Saved
    read_only = [bool]$workbook.ReadOnly
    active = [bool]$active
    sheet_count = $worksheetCount
    sheets_truncated = [bool]($worksheetCount -gt 64)
    sheets = $worksheets
  }
}
$result.workbooks = $workbooks
$result | ConvertTo-Json -Depth 8 -Compress
"#;

const WINDOWS_STATUS_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$result = [ordered]@{
  schema_version = 1
  installed = $false
  running = $false
  runtime_instance = $null
  application_version = $null
  workbooks_total = 0
  workbooks_truncated = $false
  workbook_metadata_omitted = $true
  workbooks = @()
}
$excelType = [Type]::GetTypeFromProgID('Excel.Application', $false)
if ($null -eq $excelType) {
  $result | ConvertTo-Json -Depth 4 -Compress
  exit 0
}
$result.installed = $true
try {
  $excel = [Runtime.InteropServices.Marshal]::GetActiveObject('Excel.Application')
} catch {
  $result | ConvertTo-Json -Depth 4 -Compress
  exit 0
}
$result.running = $true
try { $result.application_version = [string]$excel.Version } catch {}
try {
  $result.runtime_instance = 'hwnd:' + [string]$excel.Hwnd
  $excelProcess = Get-Process -Name EXCEL -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -eq $excel.Hwnd } |
    Select-Object -First 1
  if ($null -ne $excelProcess) { $result.runtime_instance = [string]$excelProcess.Id }
} catch {}
try {
  $result.workbooks_total = [int]$excel.Workbooks.Count
  $result.workbooks_truncated = $result.workbooks_total -gt 32
} catch {}
$result | ConvertTo-Json -Depth 4 -Compress
"#;

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
    let hasFormula = false;
    let formulaHidden = false;
    let rawFormula = null;
    try { hasFormula = Boolean(cell.hasFormula()); } catch (_) {}
    try { formulaHidden = Boolean(cell.formulaHidden()); } catch (_) {}
    if (hasFormula && !formulaHidden) {
      try { rawFormula = cell.formula2(); } catch (_) {
        try { rawFormula = cell.formula(); } catch (_) {}
      }
    }
    let numberFormat = { value: null, truncated: false };
    let numberFormatUnavailable = false;
    try {
      const rawNumberFormat = cell.numberFormat();
      if (rawNumberFormat === null || rawNumberFormat === undefined) numberFormatUnavailable = true;
      else numberFormat = boundedIdentityText(rawNumberFormat);
    } catch (_) { numberFormatUnavailable = true; }
    let rawValue = null;
    try { rawValue = cell.value2(); } catch (_) {}
    let rawDisplay = "";
    try { rawDisplay = cell.stringValue(); } catch (_) {}
    const displayed = boundedText(rawDisplay === null || rawDisplay === undefined ? "" : rawDisplay);
    const isError = hasFormula && typeof rawValue !== "string" &&
      /^(?:#NULL!|#DIV\/0!|#VALUE!|#REF!|#NAME\?|#NUM!|#N\/A|#GETTING_DATA|#SPILL!|#CALC!|#FIELD!|#BLOCKED!|#UNKNOWN!|#CONNECT!|#BUSY!|#PYTHON!)/i.test(displayed.value);
    const value = isError ? { value: null, truncated: false } : safeScalar(rawValue);
    let formula = null;
    let formulaTruncated = false;
    let formulaExternalReference = false;
    if (hasFormula && !formulaHidden && rawFormula !== null && rawFormula !== undefined) {
      const formulaText = String(rawFormula);
      formulaExternalReference = externalFormula(formulaText);
      if (!formulaExternalReference) {
        const boundedFormula = boundedText(formulaText);
        formula = boundedFormula.value;
        formulaTruncated = boundedFormula.truncated;
      }
    }
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
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$requestText = [Console]::In.ReadToEnd()
$request = $requestText | ConvertFrom-Json

function Get-ExcelRuntimeInstance($excel) {
  $runtime = 'hwnd:' + [string]$excel.Hwnd
  $process = Get-Process -Name EXCEL -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -eq $excel.Hwnd } |
    Select-Object -First 1
  if ($null -ne $process) { return [string]$process.Id }
  return $runtime
}

function Convert-BoundedText($raw) {
  $clean = ([string]$raw) -replace '[\x00-\x1F\x7F]', [char]0xFFFD
  $truncated = $clean.Length -gt 128
  if ($truncated) { $clean = $clean.Substring(0, 128) }
  return [ordered]@{ value = $clean; truncated = $truncated }
}

function Convert-BoundedIdentityText($raw) {
  $source = [string]$raw
  $lossy = $source -match '[\x00-\x1F\x7F]'
  $clean = $source -replace '[\x00-\x1F\x7F]', [char]0xFFFD
  $truncated = $lossy -or $clean.Length -gt 128
  if ($clean.Length -gt 128) { $clean = $clean.Substring(0, 128) }
  return [ordered]@{ value = $clean; truncated = $truncated }
}

function Test-ExternalFormula([string]$formula) {
  return $formula -match '\[[^\]]+\][^!]*!' -or
    $formula -match '(?i)(https?|file)://' -or
    $formula -match '\\\\' -or
    $formula -match '[A-Za-z]:\\'
}

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
    $hasFormula = $false
    $formulaHidden = $false
    try { $hasFormula = [bool]$cell.HasFormula } catch {}
    try { $formulaHidden = [bool]$cell.FormulaHidden } catch {}
    $rawFormula = $null
    if ($hasFormula -and -not $formulaHidden) {
      try { $rawFormula = $cell.Formula2 } catch {
        try { $rawFormula = $cell.Formula } catch {}
      }
    }
    $numberFormat = [ordered]@{ value = $null; truncated = $false }
    $numberFormatUnavailable = $false
    try {
      if ($null -eq $cell.NumberFormat) { $numberFormatUnavailable = $true }
      else { $numberFormat = Convert-BoundedIdentityText $cell.NumberFormat }
    } catch { $numberFormatUnavailable = $true }
    try { $rawValue = $cell.Value2 } catch { $rawValue = $null }
    try { $rawDisplay = [string]$cell.Text } catch { $rawDisplay = '' }
    $displayed = Convert-BoundedText $rawDisplay
    $isError = [bool]($hasFormula -and -not ($rawValue -is [string]) -and
      $displayed.value -match '^(?i:#NULL!|#DIV/0!|#VALUE!|#REF!|#NAME\?|#NUM!|#N/A|#GETTING_DATA|#SPILL!|#CALC!|#FIELD!|#BLOCKED!|#UNKNOWN!|#CONNECT!|#BUSY!|#PYTHON!)')
    $value = $null
    $valueTruncated = $false
    if (-not $isError -and $null -ne $rawValue) {
      if ($rawValue -is [string]) {
        $boundedValue = Convert-BoundedText $rawValue
        $value = $boundedValue.value
        $valueTruncated = $boundedValue.truncated
      } elseif ($rawValue -is [bool] -or
                $rawValue -is [byte] -or $rawValue -is [sbyte] -or
                $rawValue -is [int16] -or $rawValue -is [uint16] -or
                $rawValue -is [int32] -or $rawValue -is [uint32] -or
                $rawValue -is [int64] -or $rawValue -is [uint64] -or
                $rawValue -is [single] -or $rawValue -is [double] -or
                $rawValue -is [decimal]) {
        $value = $rawValue
      }
    }
    $formula = $null
    $formulaTruncated = $false
    $formulaExternalReference = $false
    if ($hasFormula -and -not $formulaHidden -and $null -ne $rawFormula) {
      $formulaText = [string]$rawFormula
      $formulaExternalReference = Test-ExternalFormula $formulaText
      if (-not $formulaExternalReference) {
        $boundedFormula = Convert-BoundedText $formulaText
        $formula = $boundedFormula.value
        $formulaTruncated = $boundedFormula.truncated
      }
    }
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

const MACOS_RANGE_WRITE_SCRIPT: &str = r#"
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
    return { value: clean.slice(0, 128).join(""), truncated: clean.length > 128 };
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

  function sheetVisibility(raw) {
    const number = Number(raw);
    if (number === -1) return "visible";
    if (number === 0) return "hidden";
    if (number === 2) return "very_hidden";
    const text = String(raw).toLowerCase();
    if (text.indexOf("very") >= 0 && text.indexOf("hidden") >= 0) return "very_hidden";
    if (text.indexOf("hidden") >= 0) return "hidden";
    if (text.indexOf("visible") >= 0) return "visible";
    return "unknown";
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
    let readOnly = true;
    try { readOnly = Boolean(workbook.readOnly()); } catch (_) {}
    if (workbookName !== request.workbook_name ||
        workbookFullName !== request.workbook_identity_source ||
        readOnly !== request.workbook_read_only || readOnly) {
      throw new Error("Excel workbook identity or writable state is stale");
    }
    const worksheets = workbook.worksheets();
    if (request.worksheet_index < 1 || request.worksheet_index > worksheets.length) {
      throw new Error("Excel worksheet position is stale");
    }
    const worksheet = worksheets[request.worksheet_index - 1];
    let protectedContents = true;
    let visibility = "unknown";
    try { protectedContents = Boolean(worksheet.protectContents()); } catch (_) {}
    try { visibility = sheetVisibility(worksheet.visible()); } catch (_) {}
    if (String(worksheet.name()) !== request.worksheet_name ||
        protectedContents !== request.worksheet_protected || protectedContents ||
        visibility !== request.worksheet_visibility || visibility !== "visible") {
      throw new Error("Excel worksheet identity or writable state is stale");
    }
    return { workbook: workbook, worksheet: worksheet };
  }

  function exactRange(worksheet) {
    const targetRange = worksheet.ranges.byName(request.range_address);
    if (Number(targetRange.firstRowIndex()) !== request.start_row ||
        Number(targetRange.firstColumnIndex()) !== request.start_column ||
        targetRange.rows().length !== request.row_count ||
        targetRange.columns().length !== request.column_count) {
      throw new Error("Excel returned a non-exact range");
    }
    const cells = targetRange.cells();
    if (cells.length !== request.cell_count) throw new Error("Excel returned an unexpected cell count");
    return { range: targetRange, cells: cells };
  }

  function cellState(cell, index) {
    let hasFormula = false;
    let formulaHidden = false;
    let rawFormula = null;
    try { hasFormula = Boolean(cell.hasFormula()); } catch (_) {}
    try { formulaHidden = Boolean(cell.formulaHidden()); } catch (_) {}
    if (hasFormula && !formulaHidden) {
      try { rawFormula = cell.formula2(); } catch (_) {
        try { rawFormula = cell.formula(); } catch (_) {}
      }
    }
    let numberFormat = { value: null, truncated: false };
    let numberFormatUnavailable = false;
    try {
      const rawNumberFormat = cell.numberFormat();
      if (rawNumberFormat === null || rawNumberFormat === undefined) numberFormatUnavailable = true;
      else numberFormat = boundedIdentityText(rawNumberFormat);
    } catch (_) { numberFormatUnavailable = true; }
    let rawValue = null;
    try { rawValue = cell.value2(); } catch (_) {}
    let rawDisplay = "";
    try { rawDisplay = cell.stringValue(); } catch (_) {}
    const displayed = boundedText(rawDisplay === null || rawDisplay === undefined ? "" : rawDisplay);
    const isError = hasFormula && typeof rawValue !== "string" &&
      /^(?:#NULL!|#DIV\/0!|#VALUE!|#REF!|#NAME\?|#NUM!|#N\/A|#GETTING_DATA|#SPILL!|#CALC!|#FIELD!|#BLOCKED!|#UNKNOWN!|#CONNECT!|#BUSY!|#PYTHON!)/i.test(displayed.value);
    const value = isError ? { value: null, truncated: false } : safeScalar(rawValue);
    let formula = null;
    let formulaTruncated = false;
    let formulaExternalReference = false;
    if (hasFormula && !formulaHidden && rawFormula !== null && rawFormula !== undefined) {
      const formulaText = String(rawFormula);
      formulaExternalReference = externalFormula(formulaText);
      if (!formulaExternalReference) {
        const boundedFormula = boundedText(formulaText);
        formula = boundedFormula.value;
        formulaTruncated = boundedFormula.truncated;
      }
    }
    const status = isError ? "error" : hasFormula ? "formula" :
      ((value.value === null || value.value === "") && displayed.value === "" ? "blank" : "value");
    return {
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
      is_error: isError,
      status: status
    };
  }

  function readCells(cells) {
    const states = [];
    for (let index = 0; index < cells.length; index += 1) states.push(cellState(cells[index], index));
    return states;
  }

  function sameScalar(left, right) {
    if (typeof left === "number" && typeof right === "number") return Object.is(left, right);
    return left === right;
  }

  function sameExpected(expected, actual) {
    return sameScalar(expected.value, actual.value) &&
      expected.value_truncated === actual.value_truncated &&
      expected.displayed_text === actual.displayed_text &&
      expected.displayed_text_truncated === actual.displayed_text_truncated &&
      expected.has_formula === actual.has_formula &&
      expected.formula === actual.formula &&
      expected.formula_truncated === actual.formula_truncated &&
      expected.formula_hidden === actual.formula_hidden &&
      expected.formula_external_reference === actual.formula_external_reference &&
      expected.number_format === actual.number_format &&
      expected.number_format_truncated === actual.number_format_truncated &&
      expected.number_format_unavailable === actual.number_format_unavailable &&
      expected.status === actual.status;
  }

  function sameContent(expected, actual) {
    return sameScalar(expected.value, actual.value) &&
      expected.value_truncated === actual.value_truncated &&
      expected.has_formula === actual.has_formula &&
      expected.formula === actual.formula &&
      expected.formula_truncated === actual.formula_truncated &&
      expected.formula_hidden === actual.formula_hidden &&
      expected.formula_external_reference === actual.formula_external_reference &&
      expected.status === actual.status;
  }

  function sameFormat(expected, actual) {
    return expected.number_format === actual.number_format &&
      expected.number_format_truncated === actual.number_format_truncated &&
      expected.number_format_unavailable === actual.number_format_unavailable;
  }

  function writeMatches(write, actual) {
    if (write.kind === "blank") return !actual.has_formula && actual.status === "blank" && actual.value === null;
    if (write.kind === "value") return !actual.has_formula && actual.status === "value" && sameScalar(write.value, actual.value);
    if (write.kind === "formula") {
      return actual.has_formula && !actual.formula_hidden && !actual.formula_external_reference &&
        actual.formula === write.formula;
    }
    return false;
  }

  function cellHasComment(cell) {
    try {
      const comment = cell.comment();
      if (comment !== null && comment !== undefined && String(comment).length > 0) return true;
    } catch (_) {}
    return false;
  }

  function ensureSimpleCell(cell) {
    try { if (Boolean(cell.mergeCells())) throw new Error("merged cells are not writable"); } catch (error) {
      if (String(error).indexOf("merged cells") >= 0) throw error;
    }
    try { if (Boolean(cell.hasArray())) throw new Error("array formula cells are not writable"); } catch (error) {
      if (String(error).indexOf("array formula") >= 0) throw error;
    }
    if (cellHasComment(cell)) throw new Error("commented cells are not writable");
  }

  function assignCell(cell, write) {
    if (write.kind === "blank") {
      cell.clearContents();
    } else if (write.kind === "value") {
      cell.value2 = write.value;
    } else if (write.kind === "formula") {
      try { cell.formula2 = write.formula; } catch (_) { cell.formula = write.formula; }
    } else {
      throw new Error("unsupported Excel write cell kind");
    }
  }

  function restoreCell(cell, previous) {
    if (previous.has_formula) {
      try { cell.formula2 = previous.formula; } catch (_) { cell.formula = previous.formula; }
    } else if (previous.status === "blank") {
      cell.clearContents();
    } else {
      cell.value2 = previous.value;
    }
  }

  function assignNumberFormat(cell, numberFormat) {
    cell.numberFormat = numberFormat;
  }

  function restoreNumberFormat(cell, previous) {
    if (previous.number_format_unavailable || previous.number_format_truncated ||
        previous.number_format === null || previous.number_format === undefined) {
      throw new Error("previous Excel number format is not safely restorable");
    }
    cell.numberFormat = previous.number_format;
  }

  function result(status, cells) {
    return JSON.stringify({
      schema_version: 1,
      write_status: status,
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
  }

  const selected = selectAndVerify();
  const exact = exactRange(selected.worksheet);
  if (!Array.isArray(request.expected_cells) || request.expected_cells.length !== request.cell_count) {
    throw new Error("Excel write request snapshot cell count is invalid");
  }
  if (request.mutation_kind === "content" &&
      (!Array.isArray(request.write_cells) || request.write_cells.length !== request.cell_count)) {
    throw new Error("Excel content write request cell count is invalid");
  }
  if (request.mutation_kind === "number_format" &&
      (typeof request.number_format !== "string" || request.number_format.length < 1 ||
       request.number_format.length > 128)) {
    throw new Error("Excel number format request is invalid");
  }
  if (request.mutation_kind !== "content" && request.mutation_kind !== "number_format") {
    throw new Error("Excel mutation kind is unsupported");
  }
  const before = readCells(exact.cells);
  for (let index = 0; index < exact.cells.length; index += 1) {
    ensureSimpleCell(exact.cells[index]);
    if (!sameExpected(request.expected_cells[index], before[index])) {
      throw new Error("Excel range snapshot is stale");
    }
  }
  selectAndVerify();

  let mutated = false;
  try {
    for (let index = 0; index < exact.cells.length; index += 1) {
      mutated = true;
      if (request.mutation_kind === "content") {
        assignCell(exact.cells[index], request.write_cells[index]);
      } else {
        assignNumberFormat(exact.cells[index], request.number_format);
      }
    }
    selectAndVerify();
    const written = readCells(exact.cells);
    for (let index = 0; index < written.length; index += 1) {
      if (request.mutation_kind === "content") {
        if (!writeMatches(request.write_cells[index], written[index]) ||
            !sameFormat(before[index], written[index])) {
          throw new Error("Excel content write verification failed");
        }
      } else if (!sameContent(before[index], written[index]) ||
                 written[index].number_format !== request.number_format ||
                 written[index].number_format_truncated ||
                 written[index].number_format_unavailable) {
        throw new Error("Excel number format verification failed");
      }
    }
    selectAndVerify();
    return result(request.mutation_kind === "content" ? "written" : "formatted", written);
  } catch (_) {
    if (!mutated) throw new Error("Excel write failed before mutation");
    try {
      selectAndVerify();
      for (let index = 0; index < exact.cells.length; index += 1) {
        if (request.mutation_kind === "content") restoreCell(exact.cells[index], before[index]);
        else restoreNumberFormat(exact.cells[index], before[index]);
      }
      selectAndVerify();
      const restored = readCells(exact.cells);
      for (let index = 0; index < restored.length; index += 1) {
        if (!sameExpected(request.expected_cells[index], restored[index])) {
          return result("rollback_failed", []);
        }
      }
      return result("rolled_back", restored);
    } catch (_) {
      return result("rollback_failed", []);
    }
  }
})()
"#;

const WINDOWS_RANGE_WRITE_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
[Console]::InputEncoding = $utf8
[Console]::OutputEncoding = $utf8
$requestText = [Console]::In.ReadToEnd()
$request = $requestText | ConvertFrom-Json

function Get-ExcelRuntimeInstance($excel) {
  $runtime = 'hwnd:' + [string]$excel.Hwnd
  $process = Get-Process -Name EXCEL -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -eq $excel.Hwnd } |
    Select-Object -First 1
  if ($null -ne $process) { return [string]$process.Id }
  return $runtime
}

function Convert-BoundedText($raw) {
  $clean = ([string]$raw) -replace '[\x00-\x1F\x7F]', [char]0xFFFD
  $truncated = $clean.Length -gt 128
  if ($truncated) { $clean = $clean.Substring(0, 128) }
  return [ordered]@{ value = $clean; truncated = $truncated }
}

function Convert-BoundedIdentityText($raw) {
  $source = [string]$raw
  $lossy = $source -match '[\x00-\x1F\x7F]'
  $clean = $source -replace '[\x00-\x1F\x7F]', [char]0xFFFD
  $truncated = $lossy -or $clean.Length -gt 128
  if ($clean.Length -gt 128) { $clean = $clean.Substring(0, 128) }
  return [ordered]@{ value = $clean; truncated = $truncated }
}

function Test-ExternalFormula([string]$formula) {
  return $formula -match '\[[^\]]+\][^!]*!' -or
    $formula -match '(?i)(https?|file)://' -or
    $formula -match '\\\\' -or
    $formula -match '[A-Za-z]:\\'
}

function Get-SheetVisibility($raw) {
  switch ([int]$raw) {
    -1 { return 'visible' }
    0 { return 'hidden' }
    2 { return 'very_hidden' }
    default { return 'unknown' }
  }
}

function Select-AndVerify($excel, $request) {
  if ((Get-ExcelRuntimeInstance $excel) -cne [string]$request.runtime_instance) {
    throw 'Excel process identity changed'
  }
  $workbookIndex = [int]$request.workbook_index
  if ($workbookIndex -lt 1 -or $workbookIndex -gt [int]$excel.Workbooks.Count) {
    throw 'Excel workbook position is stale'
  }
  $workbook = $excel.Workbooks.Item($workbookIndex)
  $workbookName = [string]$workbook.Name
  try { $workbookFullName = [string]$workbook.FullName } catch { $workbookFullName = $workbookName }
  $readOnly = [bool]$workbook.ReadOnly
  if ($workbookName -cne [string]$request.workbook_name -or
      $workbookFullName -cne [string]$request.workbook_identity_source -or
      $readOnly -ne [bool]$request.workbook_read_only -or $readOnly) {
    throw 'Excel workbook identity or writable state is stale'
  }
  $worksheetIndex = [int]$request.worksheet_index
  if ($worksheetIndex -lt 1 -or $worksheetIndex -gt [int]$workbook.Worksheets.Count) {
    throw 'Excel worksheet position is stale'
  }
  $worksheet = $workbook.Worksheets.Item($worksheetIndex)
  $protected = [bool]$worksheet.ProtectContents
  $visibility = Get-SheetVisibility $worksheet.Visible
  if ([string]$worksheet.Name -cne [string]$request.worksheet_name -or
      $protected -ne [bool]$request.worksheet_protected -or $protected -or
      $visibility -cne [string]$request.worksheet_visibility -or $visibility -cne 'visible') {
    throw 'Excel worksheet identity or writable state is stale'
  }
  return [ordered]@{ workbook = $workbook; worksheet = $worksheet }
}

function Get-ExactRange($worksheet, $request) {
  $range = $worksheet.Range([string]$request.range_address)
  if ([int]$range.Row -ne [int]$request.start_row -or
      [int]$range.Column -ne [int]$request.start_column -or
      [int]$range.Rows.Count -ne [int]$request.row_count -or
      [int]$range.Columns.Count -ne [int]$request.column_count -or
      [int]$range.Cells.Count -ne [int]$request.cell_count) {
    throw 'Excel returned a non-exact range'
  }
  return $range
}

function Get-CellState($cell, [int]$rowOffset, [int]$columnOffset) {
  $hasFormula = $false
  $formulaHidden = $false
  try { $hasFormula = [bool]$cell.HasFormula } catch {}
  try { $formulaHidden = [bool]$cell.FormulaHidden } catch {}
  $rawFormula = $null
  if ($hasFormula -and -not $formulaHidden) {
    try { $rawFormula = $cell.Formula2 } catch {
      try { $rawFormula = $cell.Formula } catch {}
    }
  }
  $numberFormat = [ordered]@{ value = $null; truncated = $false }
  $numberFormatUnavailable = $false
  try {
    if ($null -eq $cell.NumberFormat) { $numberFormatUnavailable = $true }
    else { $numberFormat = Convert-BoundedIdentityText $cell.NumberFormat }
  } catch { $numberFormatUnavailable = $true }
  try { $rawValue = $cell.Value2 } catch { $rawValue = $null }
  try { $rawDisplay = [string]$cell.Text } catch { $rawDisplay = '' }
  $displayed = Convert-BoundedText $rawDisplay
  $isError = [bool]($hasFormula -and -not ($rawValue -is [string]) -and
    $displayed.value -match '^(?i:#NULL!|#DIV/0!|#VALUE!|#REF!|#NAME\?|#NUM!|#N/A|#GETTING_DATA|#SPILL!|#CALC!|#FIELD!|#BLOCKED!|#UNKNOWN!|#CONNECT!|#BUSY!|#PYTHON!)')
  $value = $null
  $valueTruncated = $false
  if (-not $isError -and $null -ne $rawValue) {
    if ($rawValue -is [string]) {
      $boundedValue = Convert-BoundedText $rawValue
      $value = $boundedValue.value
      $valueTruncated = $boundedValue.truncated
    } elseif ($rawValue -is [bool] -or
              $rawValue -is [byte] -or $rawValue -is [sbyte] -or
              $rawValue -is [int16] -or $rawValue -is [uint16] -or
              $rawValue -is [int32] -or $rawValue -is [uint32] -or
              $rawValue -is [int64] -or $rawValue -is [uint64] -or
              $rawValue -is [single] -or $rawValue -is [double] -or
              $rawValue -is [decimal]) {
      $value = $rawValue
    }
  }
  $formula = $null
  $formulaTruncated = $false
  $formulaExternalReference = $false
  if ($hasFormula -and -not $formulaHidden -and $null -ne $rawFormula) {
    $formulaText = [string]$rawFormula
    $formulaExternalReference = Test-ExternalFormula $formulaText
    if (-not $formulaExternalReference) {
      $boundedFormula = Convert-BoundedText $formulaText
      $formula = $boundedFormula.value
      $formulaTruncated = $boundedFormula.truncated
    }
  }
  $status = if ($isError) { 'error' } elseif ($hasFormula) { 'formula' } elseif (
    ($null -eq $value -or $value -eq '') -and $displayed.value -eq '') { 'blank' } else { 'value' }
  return [ordered]@{
    row_offset = $rowOffset
    column_offset = $columnOffset
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
    status = $status
  }
}

function Read-Cells($range, $request) {
  $states = @()
  for ($row = 1; $row -le [int]$request.row_count; $row += 1) {
    for ($column = 1; $column -le [int]$request.column_count; $column += 1) {
      $states += Get-CellState ($range.Cells.Item($row, $column)) ($row - 1) ($column - 1)
    }
  }
  return @($states)
}

function Test-SameScalar($left, $right) {
  $leftNumber = $left -is [byte] -or $left -is [sbyte] -or
    $left -is [int16] -or $left -is [uint16] -or $left -is [int32] -or
    $left -is [uint32] -or $left -is [int64] -or $left -is [uint64] -or
    $left -is [single] -or $left -is [double] -or $left -is [decimal]
  $rightNumber = $right -is [byte] -or $right -is [sbyte] -or
    $right -is [int16] -or $right -is [uint16] -or $right -is [int32] -or
    $right -is [uint32] -or $right -is [int64] -or $right -is [uint64] -or
    $right -is [single] -or $right -is [double] -or $right -is [decimal]
  if ($leftNumber -and $rightNumber) { return [double]$left -eq [double]$right }
  if ($null -eq $left -or $null -eq $right) { return $null -eq $left -and $null -eq $right }
  return $left.GetType() -eq $right.GetType() -and $left -ceq $right
}

function Test-ExpectedCell($expected, $actual) {
  return (Test-SameScalar $expected.value $actual.value) -and
    [bool]$expected.value_truncated -eq [bool]$actual.value_truncated -and
    [string]$expected.displayed_text -ceq [string]$actual.displayed_text -and
    [bool]$expected.displayed_text_truncated -eq [bool]$actual.displayed_text_truncated -and
    [bool]$expected.has_formula -eq [bool]$actual.has_formula -and
    (($null -eq $expected.formula -and $null -eq $actual.formula) -or
      [string]$expected.formula -ceq [string]$actual.formula) -and
    [bool]$expected.formula_truncated -eq [bool]$actual.formula_truncated -and
    [bool]$expected.formula_hidden -eq [bool]$actual.formula_hidden -and
    [bool]$expected.formula_external_reference -eq [bool]$actual.formula_external_reference -and
    (($null -eq $expected.number_format -and $null -eq $actual.number_format) -or
      [string]$expected.number_format -ceq [string]$actual.number_format) -and
    [bool]$expected.number_format_truncated -eq [bool]$actual.number_format_truncated -and
    [bool]$expected.number_format_unavailable -eq [bool]$actual.number_format_unavailable -and
    [string]$expected.status -ceq [string]$actual.status
}

function Test-SameContent($expected, $actual) {
  return (Test-SameScalar $expected.value $actual.value) -and
    [bool]$expected.value_truncated -eq [bool]$actual.value_truncated -and
    [bool]$expected.has_formula -eq [bool]$actual.has_formula -and
    (($null -eq $expected.formula -and $null -eq $actual.formula) -or
      [string]$expected.formula -ceq [string]$actual.formula) -and
    [bool]$expected.formula_truncated -eq [bool]$actual.formula_truncated -and
    [bool]$expected.formula_hidden -eq [bool]$actual.formula_hidden -and
    [bool]$expected.formula_external_reference -eq [bool]$actual.formula_external_reference -and
    [string]$expected.status -ceq [string]$actual.status
}

function Test-SameFormat($expected, $actual) {
  return (($null -eq $expected.number_format -and $null -eq $actual.number_format) -or
      [string]$expected.number_format -ceq [string]$actual.number_format) -and
    [bool]$expected.number_format_truncated -eq [bool]$actual.number_format_truncated -and
    [bool]$expected.number_format_unavailable -eq [bool]$actual.number_format_unavailable
}

function Test-WriteMatches($write, $actual) {
  if ([string]$write.kind -ceq 'blank') {
    return -not [bool]$actual.has_formula -and [string]$actual.status -ceq 'blank' -and $null -eq $actual.value
  }
  if ([string]$write.kind -ceq 'value') {
    return -not [bool]$actual.has_formula -and [string]$actual.status -ceq 'value' -and
      (Test-SameScalar $write.value $actual.value)
  }
  if ([string]$write.kind -ceq 'formula') {
    return [bool]$actual.has_formula -and -not [bool]$actual.formula_hidden -and
      -not [bool]$actual.formula_external_reference -and
      [string]$actual.formula -ceq [string]$write.formula
  }
  return $false
}

function Assert-SimpleCell($cell) {
  try { if ([bool]$cell.MergeCells) { throw 'merged cells are not writable' } } catch {
    if ([string]$_ -match 'merged cells') { throw }
  }
  try { if ([bool]$cell.HasArray) { throw 'array formula cells are not writable' } } catch {
    if ([string]$_ -match 'array formula') { throw }
  }
  try { if ($null -ne $cell.Comment) { throw 'commented cells are not writable' } } catch {
    if ([string]$_ -match 'commented cells') { throw }
  }
  try { if ($null -ne $cell.CommentThreaded) { throw 'threaded-comment cells are not writable' } } catch {
    if ([string]$_ -match 'threaded-comment') { throw }
  }
}

function Set-WriteCell($cell, $write) {
  switch ([string]$write.kind) {
    'blank' { $null = $cell.ClearContents(); return }
    'value' { $cell.Value2 = $write.value; return }
    'formula' {
      try { $cell.Formula2 = [string]$write.formula } catch { $cell.Formula = [string]$write.formula }
      return
    }
    default { throw 'unsupported Excel write cell kind' }
  }
}

function Restore-Cell($cell, $previous) {
  if ([bool]$previous.has_formula) {
    try { $cell.Formula2 = [string]$previous.formula } catch { $cell.Formula = [string]$previous.formula }
  } elseif ([string]$previous.status -ceq 'blank') {
    $null = $cell.ClearContents()
  } else {
    $cell.Value2 = $previous.value
  }
}

function Set-NumberFormat($cell, [string]$numberFormat) {
  $cell.NumberFormat = $numberFormat
}

function Restore-NumberFormat($cell, $previous) {
  if ([bool]$previous.number_format_unavailable -or [bool]$previous.number_format_truncated -or
      $null -eq $previous.number_format) {
    throw 'previous Excel number format is not safely restorable'
  }
  $cell.NumberFormat = [string]$previous.number_format
}

function New-Result([string]$status, $cells, $request) {
  return [ordered]@{
    schema_version = 1
    write_status = $status
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
    cells = @($cells)
  }
}

$excelType = [Type]::GetTypeFromProgID('Excel.Application', $false)
if ($null -eq $excelType) { throw 'Microsoft Excel is not installed' }
$excel = [Runtime.InteropServices.Marshal]::GetActiveObject('Excel.Application')
$selected = Select-AndVerify $excel $request
$range = Get-ExactRange $selected.worksheet $request
$expectedCells = @($request.expected_cells)
if ($expectedCells.Count -ne [int]$request.cell_count) {
  throw 'Excel write request snapshot cell count is invalid'
}
$writeCells = @()
if ([string]$request.mutation_kind -ceq 'content') {
  $writeCells = @($request.write_cells)
  if ($writeCells.Count -ne [int]$request.cell_count) {
    throw 'Excel content write request cell count is invalid'
  }
} elseif ([string]$request.mutation_kind -ceq 'number_format') {
  if ($null -eq $request.number_format -or [string]$request.number_format -eq '' -or
      ([string]$request.number_format).Length -gt 128) {
    throw 'Excel number format request is invalid'
  }
} else {
  throw 'Excel mutation kind is unsupported'
}
$before = @(Read-Cells $range $request)
$position = 0
for ($row = 1; $row -le [int]$request.row_count; $row += 1) {
  for ($column = 1; $column -le [int]$request.column_count; $column += 1) {
    $cell = $range.Cells.Item($row, $column)
    Assert-SimpleCell $cell
    if (-not (Test-ExpectedCell $expectedCells[$position] $before[$position])) {
      throw 'Excel range snapshot is stale'
    }
    $position += 1
  }
}
$null = Select-AndVerify $excel $request

$mutated = $false
try {
  $position = 0
  for ($row = 1; $row -le [int]$request.row_count; $row += 1) {
    for ($column = 1; $column -le [int]$request.column_count; $column += 1) {
      $mutated = $true
      if ([string]$request.mutation_kind -ceq 'content') {
        Set-WriteCell ($range.Cells.Item($row, $column)) ($writeCells[$position])
      } else {
        Set-NumberFormat ($range.Cells.Item($row, $column)) ([string]$request.number_format)
      }
      $position += 1
    }
  }
  $null = Select-AndVerify $excel $request
  $written = @(Read-Cells $range $request)
  for ($index = 0; $index -lt $written.Count; $index += 1) {
    if ([string]$request.mutation_kind -ceq 'content') {
      if (-not (Test-WriteMatches $writeCells[$index] $written[$index]) -or
          -not (Test-SameFormat $before[$index] $written[$index])) {
        throw 'Excel content write verification failed'
      }
    } elseif (-not (Test-SameContent $before[$index] $written[$index]) -or
              [string]$written[$index].number_format -cne [string]$request.number_format -or
              [bool]$written[$index].number_format_truncated -or
              [bool]$written[$index].number_format_unavailable) {
      throw 'Excel number format verification failed'
    }
  }
  $null = Select-AndVerify $excel $request
  $status = if ([string]$request.mutation_kind -ceq 'content') { 'written' } else { 'formatted' }
  New-Result $status $written $request | ConvertTo-Json -Depth 8 -Compress
  exit 0
} catch {
  if (-not $mutated) { throw }
  try {
    $null = Select-AndVerify $excel $request
    $position = 0
    for ($row = 1; $row -le [int]$request.row_count; $row += 1) {
      for ($column = 1; $column -le [int]$request.column_count; $column += 1) {
        if ([string]$request.mutation_kind -ceq 'content') {
          Restore-Cell ($range.Cells.Item($row, $column)) ($before[$position])
        } else {
          Restore-NumberFormat ($range.Cells.Item($row, $column)) ($before[$position])
        }
        $position += 1
      }
    }
    $null = Select-AndVerify $excel $request
    $restored = @(Read-Cells $range $request)
    for ($index = 0; $index -lt $restored.Count; $index += 1) {
      if (-not (Test-ExpectedCell $expectedCells[$index] $restored[$index])) {
        New-Result 'rollback_failed' @() $request | ConvertTo-Json -Depth 8 -Compress
        exit 0
      }
    }
    New-Result 'rolled_back' $restored $request | ConvertTo-Json -Depth 8 -Compress
    exit 0
  } catch {
    New-Result 'rollback_failed' @() $request | ConvertTo-Json -Depth 8 -Compress
    exit 0
  }
}
"#;

pub(super) fn tool_definitions(include_write: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({
            "name": "excel_live_status",
            "description": "Inspect whether Microsoft Excel is installed and already running, without launching it or reading workbook names or cell contents.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "excel_list_open_workbooks",
            "description": "List bounded metadata for workbooks already open in the current Microsoft Excel instance. Returns opaque workbook identities and never launches Excel or reads cell contents.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        }),
        json!({
            "name": "excel_inspect_workbook",
            "description": "Inspect worksheet names, visibility, protection, and active state for one exact opaque workbook identity returned by excel_list_open_workbooks. Does not read cells or mutate Excel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workbook_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque workbook identity returned by excel_list_open_workbooks."
                    }
                },
                "required": ["workbook_id"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "excel_read_range",
            "description": "Read up to 256 cells from one exact worksheet and canonical A1 range in an already-open Microsoft Excel workbook. Returns bounded scalar values, displayed text, non-hidden non-external formulas, and a safe number-format classification without activating, recalculating, or mutating Excel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workbook_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque workbook identity returned by excel_list_open_workbooks."
                    },
                    "worksheet_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque worksheet identity returned by excel_inspect_workbook."
                    },
                    "range": {
                        "type": "string",
                        "pattern": "^[A-Z]{1,3}[1-9][0-9]*(?::[A-Z]{1,3}[1-9][0-9]*)?$",
                        "maxLength": 32,
                        "description": "Canonical uppercase A1 range without a sheet name, dollar signs, unions, or whole-row/whole-column references."
                    }
                },
                "required": ["workbook_id", "worksheet_id", "range"],
                "additionalProperties": false
            }
        }),
    ];
    if include_write {
        tools.push(json!({
            "name": "excel_write_range",
            "description": "After mandatory interactive approval, replace the contents of up to 256 exact visible, unprotected cells in an already-open writable Excel workbook. Requires the exact optimistic snapshot ID from a fresh excel_read_range result; writes only typed blanks, scalar constants, or strictly allowlisted local formulas, verifies content and number-format preservation, and attempts verified rollback on partial failure. Does not save, export, activate, select, format, or explicitly recalculate Excel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workbook_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque workbook identity from the current Excel discovery snapshot."
                    },
                    "worksheet_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque worksheet identity from the current workbook inspection."
                    },
                    "range": {
                        "type": "string",
                        "pattern": "^[A-Z]{1,3}[1-9][0-9]*(?::[A-Z]{1,3}[1-9][0-9]*)?$",
                        "maxLength": 32,
                        "description": "The exact canonical uppercase A1 range used for the fresh read snapshot."
                    },
                    "expected_snapshot_id": {
                        "type": "string",
                        "pattern": "^excel_range_[0-9a-f]{64}$",
                        "maxLength": 96,
                        "description": "Exact range_snapshot_id returned by a fresh excel_read_range for the same workbook, worksheet, and range."
                    },
                    "cells": {
                        "type": "array",
                        "minItems": 1,
                        "maxItems": 256,
                        "description": "Exact rectangular row matrix matching the target range geometry.",
                        "items": {
                            "type": "array",
                            "minItems": 1,
                            "maxItems": 256,
                            "items": {
                                "oneOf": [
                                    {
                                        "type": "object",
                                        "properties": {"kind": {"const": "blank"}},
                                        "required": ["kind"],
                                        "additionalProperties": false
                                    },
                                    {
                                        "type": "object",
                                        "properties": {
                                            "kind": {"const": "value"},
                                            "value": {"type": ["boolean", "number", "string"], "maxLength": 128}
                                        },
                                        "required": ["kind", "value"],
                                        "additionalProperties": false
                                    },
                                    {
                                        "type": "object",
                                        "properties": {
                                            "kind": {"const": "formula"},
                                            "formula": {"type": "string", "minLength": 2, "maxLength": 128, "pattern": "^="}
                                        },
                                        "required": ["kind", "formula"],
                                        "additionalProperties": false
                                    }
                                ]
                            }
                        }
                    }
                },
                "required": ["workbook_id", "worksheet_id", "range", "expected_snapshot_id", "cells"],
                "additionalProperties": false
            }
        }));
        tools.push(json!({
            "name": "excel_set_number_format",
            "description": "After mandatory interactive approval, apply one fixed allowlisted number-format preset to up to 256 exact visible, unprotected cells in an already-open writable Excel workbook. Requires the exact snapshot ID from a fresh excel_read_range, preserves cell contents and formulas, verifies the result, and attempts exact format rollback on partial failure. Does not save, export, activate, select, or explicitly recalculate Excel.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workbook_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque workbook identity from the current Excel discovery snapshot."
                    },
                    "worksheet_id": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 96,
                        "description": "Exact opaque worksheet identity from the current workbook inspection."
                    },
                    "range": {
                        "type": "string",
                        "pattern": "^[A-Z]{1,3}[1-9][0-9]*(?::[A-Z]{1,3}[1-9][0-9]*)?$",
                        "maxLength": 32,
                        "description": "The exact canonical uppercase A1 range used for the fresh read snapshot."
                    },
                    "expected_snapshot_id": {
                        "type": "string",
                        "pattern": "^excel_range_[0-9a-f]{64}$",
                        "maxLength": 96,
                        "description": "Exact range_snapshot_id returned by a fresh excel_read_range for the same workbook, worksheet, and range."
                    },
                    "preset": {
                        "type": "string",
                        "enum": ["general", "integer", "decimal_2", "percent_2", "date", "datetime", "text"],
                        "description": "Fixed locale-independent number-format preset; arbitrary custom format strings are not accepted."
                    }
                },
                "required": ["workbook_id", "worksheet_id", "range", "expected_snapshot_id", "preset"],
                "additionalProperties": false
            }
        }));
    }
    tools
}

pub(super) fn requires_interactive_approval(operation: &str) -> bool {
    matches!(operation, "excel_write_range" | "excel_set_number_format")
}

pub(super) fn approval_command(
    operation: &str,
    arguments: &Value,
) -> Result<(String, Vec<String>)> {
    let args = match operation {
        "excel_write_range" => {
            let input = parse_range_write_input(arguments)?;
            let summary = write_cell_summary(input.cells.as_slice())?;
            vec![
                "write_range".to_string(),
                format!("workbook_id={}", input.workbook_id),
                format!("worksheet_id={}", input.worksheet_id),
                format!("range={}", input.range.canonical),
                format!("expected_snapshot_id={}", input.expected_snapshot_id),
                format!("cell_count={}", input.range.cell_count),
                format!("blank_cells={}", summary.blank_cells),
                format!("value_cells={}", summary.value_cells),
                format!("formula_cells={}", summary.formula_cells),
                format!("text_characters={}", summary.text_characters),
                format!("content_sha256={}", summary.content_sha256),
            ]
        }
        "excel_set_number_format" => {
            let input = parse_range_format_input(arguments)?;
            vec![
                "set_number_format".to_string(),
                format!("workbook_id={}", input.workbook_id),
                format!("worksheet_id={}", input.worksheet_id),
                format!("range={}", input.range.canonical),
                format!("expected_snapshot_id={}", input.expected_snapshot_id),
                format!("cell_count={}", input.range.cell_count),
                format!("preset={}", input.preset),
            ]
        }
        _ => bail!("Excel Live Control operation does not support interactive approval"),
    };
    Ok(("chatos-excel-live".to_string(), args))
}

pub(super) fn execute_approved(
    operation: &str,
    arguments: &Value,
    approved_command_args: Option<&[String]>,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if !requires_interactive_approval(operation) {
        bail!("Excel Live Control operation does not support approved execution");
    }
    let (_, expected) = approval_command(operation, arguments)?;
    if approved_command_args != Some(expected.as_slice()) {
        bail!("approved Excel write no longer matches the exact reviewed arguments");
    }
    match operation {
        "excel_write_range" => execute_range_write(arguments, action_cancelled),
        "excel_set_number_format" => execute_range_format_write(arguments, action_cancelled),
        _ => unreachable!("approval-gated Excel operation was already checked"),
    }
}

pub(super) fn dependency_error() -> Option<String> {
    match std::env::consts::OS {
        "macos" if !regular_non_symlink_file(Path::new(MACOS_OSASCRIPT_PATH)) => Some(
            "Excel Live Control requires the system osascript automation bridge on macOS"
                .to_string(),
        ),
        "macos" => None,
        "windows" => windows_powershell_path()
            .err()
            .map(|error| error.to_string()),
        _ => {
            Some("Excel Live Control is currently available only on macOS and Windows".to_string())
        }
    }
}

pub(super) fn execute(operation: &str, arguments: &Value) -> Result<Value> {
    if requires_interactive_approval(operation) {
        bail!(
            "Excel live range mutations require the signed Plugin runtime and interactive approval"
        );
    }
    if operation == "excel_read_range" {
        return execute_range_read(arguments);
    }
    let snapshot = if operation == "excel_live_status" {
        read_platform_status()?
    } else {
        read_platform_snapshot()?
    };
    execute_with_snapshot(operation, arguments, snapshot)
}

fn execute_range_read(arguments: &Value) -> Result<Value> {
    ensure_exact_arguments(arguments, &["workbook_id", "worksheet_id", "range"])?;
    let workbook_id = required_text(arguments, "workbook_id", 96)?;
    let worksheet_id = required_text(arguments, "worksheet_id", 96)?;
    let range = parse_a1_range(required_text(arguments, "range", 32)?)?;
    let before = read_platform_snapshot()?;
    let normalized_before = normalize_snapshot(before.clone())?;
    if !normalized_before
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(excel_not_running_error(&normalized_before));
    }
    let target = resolve_range_read_target(&before, &normalized_before, workbook_id, worksheet_id)?;
    let request = range_read_bridge_request(&target, &range);
    let response = read_platform_range(&request)?;
    let cells = normalize_range_read_response(response, &target, &range)?;

    let after = read_platform_snapshot()?;
    let normalized_after = normalize_snapshot(after.clone())?;
    let refreshed = resolve_range_read_target(
        &after,
        &normalized_after,
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
    )?;
    if refreshed != target {
        bail!("Excel workbook or worksheet identity changed during the bounded read; inspect it again");
    }

    Ok(range_read_response(&target, &range, cells))
}

fn execute_range_write(arguments: &Value, action_cancelled: Option<&AtomicBool>) -> Result<Value> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel write was cancelled before execution");
    }
    let _write_guard = EXCEL_WRITE_LOCK
        .lock()
        .map_err(|_| anyhow!("Excel live write lock is unavailable"))?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel write was cancelled while waiting for another write");
    }
    let input = parse_range_write_input(arguments)?;
    let before = read_platform_snapshot()?;
    let normalized_before = normalize_snapshot(before.clone())?;
    if !normalized_before
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(excel_not_running_error(&normalized_before));
    }
    let target = resolve_range_read_target(
        &before,
        &normalized_before,
        input.workbook_id.as_str(),
        input.worksheet_id.as_str(),
    )?;
    ensure_write_target_is_mutable(&target)?;

    let current = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let current_cells = normalize_range_read_response(current, &target, &input.range)?;
    let current_snapshot_id = range_snapshot_id(&target, &input.range, current_cells.as_slice());
    if current_snapshot_id != input.expected_snapshot_id {
        bail!("Excel range changed after it was read; read the exact range again before writing");
    }
    ensure_snapshot_cells_are_write_safe(current_cells.as_slice())?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel write was cancelled before mutation");
    }

    let request = range_write_bridge_request(&target, &input, current_cells.as_slice());
    let response = write_platform_range(&request)?;
    normalize_range_write_response(response, &target, &input, current_cells.as_slice())?;

    let after = read_platform_snapshot()?;
    let normalized_after = normalize_snapshot(after.clone())?;
    let refreshed = resolve_range_read_target(
        &after,
        &normalized_after,
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
    )?;
    if refreshed != target {
        bail!("Excel workbook or worksheet identity changed after the verified write; inspect it again");
    }

    let final_response = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let final_cells = normalize_range_read_response(final_response, &target, &input.range)?;
    if !desired_cells_match(input.cells.as_slice(), final_cells.as_slice())?
        || !same_number_formats(current_cells.as_slice(), final_cells.as_slice())?
    {
        bail!("Excel range changed after the bridge verified the write; inspect the range before any retry");
    }
    Ok(range_write_response(
        &target,
        &input.range,
        final_cells,
        action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)),
    ))
}

fn execute_range_format_write(
    arguments: &Value,
    action_cancelled: Option<&AtomicBool>,
) -> Result<Value> {
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel number format write was cancelled before execution");
    }
    let _write_guard = EXCEL_WRITE_LOCK
        .lock()
        .map_err(|_| anyhow!("Excel live write lock is unavailable"))?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel number format write was cancelled while waiting for another write");
    }
    let input = parse_range_format_input(arguments)?;
    let before = read_platform_snapshot()?;
    let normalized_before = normalize_snapshot(before.clone())?;
    if !normalized_before
        .get("running")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return Err(excel_not_running_error(&normalized_before));
    }
    let target = resolve_range_read_target(
        &before,
        &normalized_before,
        input.workbook_id.as_str(),
        input.worksheet_id.as_str(),
    )?;
    ensure_write_target_is_mutable(&target)?;

    let current = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let current_cells = normalize_range_read_response(current, &target, &input.range)?;
    let current_snapshot_id = range_snapshot_id(&target, &input.range, current_cells.as_slice());
    if current_snapshot_id != input.expected_snapshot_id {
        bail!(
            "Excel range changed after it was read; read the exact range again before formatting"
        );
    }
    ensure_snapshot_cells_are_format_safe(current_cells.as_slice())?;
    if action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)) {
        bail!("approved Excel number format write was cancelled before mutation");
    }

    let request = range_format_bridge_request(&target, &input, current_cells.as_slice());
    let response = write_platform_range(&request)?;
    normalize_range_format_response(response, &target, &input, current_cells.as_slice())?;

    let after = read_platform_snapshot()?;
    let normalized_after = normalize_snapshot(after.clone())?;
    let refreshed = resolve_range_read_target(
        &after,
        &normalized_after,
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
    )?;
    if refreshed != target {
        bail!("Excel workbook or worksheet identity changed after the verified number format write; inspect it again");
    }

    let final_response = read_platform_range(&range_read_bridge_request(&target, &input.range))?;
    let final_cells = normalize_range_read_response(final_response, &target, &input.range)?;
    if !formatted_cells_match(
        current_cells.as_slice(),
        final_cells.as_slice(),
        input.number_format.as_str(),
    )? {
        bail!("Excel range changed after the bridge verified the number format; inspect the range before any retry");
    }
    Ok(range_format_response(
        &target,
        &input,
        final_cells,
        action_cancelled.is_some_and(|cancelled| cancelled.load(Ordering::SeqCst)),
    ))
}

fn ensure_write_target_is_mutable(target: &RangeReadTarget) -> Result<()> {
    if target.workbook_read_only {
        bail!("Excel workbook is read-only; live range writes are disabled");
    }
    if target.worksheet_protected {
        bail!("Excel worksheet is protected; live range writes are disabled");
    }
    if target.worksheet_visibility != "visible" {
        bail!("Excel live range writes require an exact visible worksheet");
    }
    Ok(())
}

fn ensure_snapshot_cells_are_format_safe(cells: &[Value]) -> Result<()> {
    for cell in cells {
        let object = cell
            .as_object()
            .expect("normalized Excel range cell object");
        let address = object
            .get("address")
            .and_then(Value::as_str)
            .expect("normalized Excel range cell address");
        if object
            .get("value_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            || object
                .get("displayed_text_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            || object
                .get("formula_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            || object
                .get("number_format_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            bail!("Excel cell {address} has truncated state and cannot be safely formatted");
        }
        if object
            .get("formula_hidden")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            || object
                .get("formula_external_reference")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            bail!("Excel cell {address} has a hidden or external formula and cannot be safely formatted");
        }
        if object
            .get("number_format_unavailable")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            || object
                .get("number_format")
                .and_then(Value::as_str)
                .is_none()
        {
            bail!("Excel cell {address} number format cannot be read and restored exactly");
        }
    }
    Ok(())
}

fn ensure_snapshot_cells_are_write_safe(cells: &[Value]) -> Result<()> {
    for cell in cells {
        let object = cell
            .as_object()
            .expect("normalized Excel range cell object");
        let address = object
            .get("address")
            .and_then(Value::as_str)
            .expect("normalized Excel range cell address");
        if object
            .get("value_truncated")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            || object
                .get("displayed_text_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            || object
                .get("formula_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true)
            || object
                .get("number_format_truncated")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            bail!("Excel cell {address} has truncated state and cannot be safely replaced");
        }
        if object
            .get("number_format_unavailable")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            || object
                .get("number_format")
                .and_then(Value::as_str)
                .is_none()
        {
            bail!(
                "Excel cell {address} number format cannot be verified before content replacement"
            );
        }
        if object
            .get("formula_hidden")
            .and_then(Value::as_bool)
            .unwrap_or(true)
            || object
                .get("formula_external_reference")
                .and_then(Value::as_bool)
                .unwrap_or(true)
        {
            bail!("Excel cell {address} has a hidden or external formula and cannot be safely replaced");
        }
        let status = object
            .get("status")
            .and_then(Value::as_str)
            .expect("normalized Excel range cell status");
        match status {
            "blank" => {}
            "value" => {
                let value = object
                    .get("value")
                    .expect("normalized Excel range cell value");
                if value.is_null() {
                    bail!("Excel cell {address} has an unsupported non-scalar value");
                }
                if let Some(value) = value.as_str() {
                    validate_safe_live_text(value, "existing Excel cell text")?;
                }
            }
            "formula" | "error" => {
                let formula = object
                    .get("formula")
                    .and_then(Value::as_str)
                    .ok_or_else(|| anyhow!("Excel cell {address} formula cannot be restored"))?;
                validate_live_formula(formula).with_context(|| {
                    format!("Excel cell {address} uses a formula outside the rollback allowlist")
                })?;
            }
            _ => bail!("Excel cell {address} has an unsupported state"),
        }
    }
    Ok(())
}

fn range_write_bridge_request(
    target: &RangeReadTarget,
    input: &RangeWriteInput,
    expected_cells: &[Value],
) -> Value {
    json!({
        "schema_version": 1,
        "mutation_kind": "content",
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "workbook_identity_source": target.workbook_identity_source,
        "workbook_read_only": target.workbook_read_only,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "worksheet_visibility": target.worksheet_visibility,
        "worksheet_protected": target.worksheet_protected,
        "range_address": input.range.canonical,
        "start_row": input.range.start_row,
        "start_column": input.range.start_column,
        "row_count": input.range.row_count,
        "column_count": input.range.column_count,
        "cell_count": input.range.cell_count,
        "expected_cells": expected_cells,
        "write_cells": write_cells_bridge_value(input.cells.as_slice()),
    })
}

fn range_format_bridge_request(
    target: &RangeReadTarget,
    input: &RangeFormatInput,
    expected_cells: &[Value],
) -> Value {
    json!({
        "schema_version": 1,
        "mutation_kind": "number_format",
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "workbook_identity_source": target.workbook_identity_source,
        "workbook_read_only": target.workbook_read_only,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "worksheet_visibility": target.worksheet_visibility,
        "worksheet_protected": target.worksheet_protected,
        "range_address": input.range.canonical,
        "start_row": input.range.start_row,
        "start_column": input.range.start_column,
        "row_count": input.range.row_count,
        "column_count": input.range.column_count,
        "cell_count": input.range.cell_count,
        "expected_cells": expected_cells,
        "number_format": input.number_format,
    })
}

fn normalize_range_write_response(
    response: Value,
    target: &RangeReadTarget,
    input: &RangeWriteInput,
    expected_cells: &[Value],
) -> Result<Vec<Value>> {
    let status = response
        .get("write_status")
        .and_then(Value::as_str)
        .context("Excel range write response is missing write_status")?
        .to_string();
    if status == "rollback_failed" {
        bail!("Excel write failed and the bridge could not verify complete rollback; inspect the workbook immediately and do not retry automatically");
    }
    let cells = normalize_range_read_response(response, target, &input.range)?;
    match status.as_str() {
        "written" => {
            if !desired_cells_match(input.cells.as_slice(), cells.as_slice())?
                || !same_number_formats(expected_cells, cells.as_slice())?
            {
                bail!(
                    "Excel bridge returned a write result that does not match the approved cells or preserve their number formats"
                );
            }
            Ok(cells)
        }
        "rolled_back" => {
            if cells != expected_cells {
                bail!("Excel write failed and rollback verification did not reproduce the exact prior snapshot");
            }
            bail!("Excel write failed after mutation, but the exact target range was restored and verified; inspect it before retrying")
        }
        _ => bail!("Excel range write response has an unsupported status"),
    }
}

fn normalize_range_format_response(
    response: Value,
    target: &RangeReadTarget,
    input: &RangeFormatInput,
    expected_cells: &[Value],
) -> Result<Vec<Value>> {
    let status = response
        .get("write_status")
        .and_then(Value::as_str)
        .context("Excel number format response is missing write_status")?
        .to_string();
    if status == "rollback_failed" {
        bail!("Excel number format write failed and the bridge could not verify complete rollback; inspect the workbook immediately and do not retry automatically");
    }
    let cells = normalize_range_read_response(response, target, &input.range)?;
    match status.as_str() {
        "formatted" => {
            if !formatted_cells_match(
                expected_cells,
                cells.as_slice(),
                input.number_format.as_str(),
            )? {
                bail!("Excel bridge returned a number format result that changed cell contents or did not match the approved preset");
            }
            Ok(cells)
        }
        "rolled_back" => {
            if cells != expected_cells {
                bail!("Excel number format write failed and rollback verification did not reproduce the exact prior snapshot");
            }
            bail!("Excel number format write failed after mutation, but the exact target range was restored and verified; inspect it before retrying")
        }
        _ => bail!("Excel number format response has an unsupported status"),
    }
}

fn same_number_formats(expected: &[Value], actual: &[Value]) -> Result<bool> {
    if expected.len() != actual.len() {
        return Ok(false);
    }
    for (expected, actual) in expected.iter().zip(actual) {
        let expected = expected
            .as_object()
            .context("normalized expected Excel cell must be an object")?;
        let actual = actual
            .as_object()
            .context("normalized actual Excel cell must be an object")?;
        if expected.get("number_format") != actual.get("number_format")
            || expected.get("number_format_truncated") != actual.get("number_format_truncated")
            || expected.get("number_format_unavailable") != actual.get("number_format_unavailable")
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn formatted_cells_match(
    expected: &[Value],
    actual: &[Value],
    number_format: &str,
) -> Result<bool> {
    if expected.len() != actual.len() {
        return Ok(false);
    }
    for (expected, actual) in expected.iter().zip(actual) {
        let expected = expected
            .as_object()
            .context("normalized expected Excel format cell must be an object")?;
        let actual = actual
            .as_object()
            .context("normalized actual Excel format cell must be an object")?;
        if !same_cell_content(expected, actual)
            || actual.get("number_format").and_then(Value::as_str) != Some(number_format)
            || actual
                .get("number_format_truncated")
                .and_then(Value::as_bool)
                != Some(false)
            || actual
                .get("number_format_unavailable")
                .and_then(Value::as_bool)
                != Some(false)
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn same_cell_content(expected: &Map<String, Value>, actual: &Map<String, Value>) -> bool {
    same_cell_scalar(
        expected.get("value").unwrap_or(&Value::Null),
        actual.get("value"),
    ) && [
        "value_truncated",
        "has_formula",
        "formula",
        "formula_truncated",
        "formula_hidden",
        "formula_external_reference",
        "status",
    ]
    .into_iter()
    .all(|field| expected.get(field) == actual.get(field))
}

fn desired_cells_match(desired: &[WriteCell], actual: &[Value]) -> Result<bool> {
    if desired.len() != actual.len() {
        return Ok(false);
    }
    for (desired, actual) in desired.iter().zip(actual) {
        let actual = actual
            .as_object()
            .context("normalized Excel write result cell must be an object")?;
        let has_formula = actual
            .get("has_formula")
            .and_then(Value::as_bool)
            .context("normalized Excel write result formula state is missing")?;
        let matches = match desired {
            WriteCell::Blank => {
                !has_formula
                    && actual.get("status").and_then(Value::as_str) == Some("blank")
                    && actual.get("value").is_some_and(Value::is_null)
            }
            WriteCell::Value(value) => {
                !has_formula
                    && actual.get("status").and_then(Value::as_str) == Some("value")
                    && same_cell_scalar(value, actual.get("value"))
            }
            WriteCell::Formula(formula) => {
                has_formula
                    && actual.get("formula").and_then(Value::as_str) == Some(formula.as_str())
                    && actual.get("formula_hidden").and_then(Value::as_bool) == Some(false)
                    && actual
                        .get("formula_external_reference")
                        .and_then(Value::as_bool)
                        == Some(false)
            }
        };
        if !matches {
            return Ok(false);
        }
    }
    Ok(true)
}

fn same_cell_scalar(expected: &Value, actual: Option<&Value>) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    match (expected, actual) {
        (Value::Number(expected), Value::Number(actual)) => expected
            .as_f64()
            .zip(actual.as_f64())
            .is_some_and(|(expected, actual)| expected.to_bits() == actual.to_bits()),
        _ => expected == actual,
    }
}

fn range_write_response(
    target: &RangeReadTarget,
    range: &A1Range,
    cells: Vec<Value>,
    cancel_requested_after_commit: bool,
) -> Value {
    let range_snapshot_id = range_snapshot_id(target, range, cells.as_slice());
    let rows = cells
        .chunks(range.column_count)
        .map(|row| Value::Array(row.iter().map(public_cell_projection).collect()))
        .collect::<Vec<_>>();
    json!({
        "platform": std::env::consts::OS,
        "excel_running": true,
        "safe_no_launch": true,
        "write_verified": true,
        "rollback_status": "not_needed",
        "save_performed": false,
        "export_performed": false,
        "explicit_recalculation_performed": false,
        "cancel_requested_after_commit": cancel_requested_after_commit,
        "range_snapshot_id": range_snapshot_id,
        "workbook": {
            "workbook_id": target.workbook_id,
            "name": target.workbook_name,
            "index": target.workbook_index,
        },
        "worksheet": {
            "worksheet_id": target.worksheet_id,
            "name": target.worksheet_name,
            "index": target.worksheet_index,
            "protected": target.worksheet_protected,
        },
        "range": {
            "address": range.canonical,
            "start_row": range.start_row,
            "start_column": range.start_column,
            "row_count": range.row_count,
            "column_count": range.column_count,
            "cell_count": range.cell_count,
        },
        "cells": rows,
    })
}

fn range_format_response(
    target: &RangeReadTarget,
    input: &RangeFormatInput,
    cells: Vec<Value>,
    cancel_requested_after_commit: bool,
) -> Value {
    let range_snapshot_id = range_snapshot_id(target, &input.range, cells.as_slice());
    let rows = cells
        .chunks(input.range.column_count)
        .map(|row| Value::Array(row.iter().map(public_cell_projection).collect()))
        .collect::<Vec<_>>();
    json!({
        "platform": std::env::consts::OS,
        "excel_running": true,
        "safe_no_launch": true,
        "format_verified": true,
        "number_format_preset": input.preset,
        "rollback_status": "not_needed",
        "save_performed": false,
        "export_performed": false,
        "explicit_recalculation_performed": false,
        "cancel_requested_after_commit": cancel_requested_after_commit,
        "range_snapshot_id": range_snapshot_id,
        "workbook": {
            "workbook_id": target.workbook_id,
            "name": target.workbook_name,
            "index": target.workbook_index,
        },
        "worksheet": {
            "worksheet_id": target.worksheet_id,
            "name": target.worksheet_name,
            "index": target.worksheet_index,
            "protected": target.worksheet_protected,
        },
        "range": {
            "address": input.range.canonical,
            "start_row": input.range.start_row,
            "start_column": input.range.start_column,
            "row_count": input.range.row_count,
            "column_count": input.range.column_count,
            "cell_count": input.range.cell_count,
        },
        "cells": rows,
    })
}

fn execute_with_snapshot(operation: &str, arguments: &Value, snapshot: Value) -> Result<Value> {
    let normalized = normalize_snapshot(snapshot)?;
    match operation {
        "excel_live_status" => {
            ensure_exact_arguments(arguments, &[])?;
            Ok(status_response(&normalized))
        }
        "excel_list_open_workbooks" => {
            ensure_exact_arguments(arguments, &[])?;
            if !normalized
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(excel_not_running_error(&normalized));
            }
            Ok(json!({
                "platform": normalized.get("platform"),
                "excel_installed": normalized.get("installed"),
                "excel_running": true,
                "read_only": true,
                "safe_no_launch": true,
                "application_version": normalized.get("application_version"),
                "workbook_count": normalized.get("workbooks_total"),
                "workbooks_truncated": normalized.get("workbooks_truncated"),
                "workbooks": normalized.get("workbooks").and_then(Value::as_array).expect("normalized Excel workbooks").iter().map(workbook_list_projection).collect::<Vec<_>>(),
            }))
        }
        "excel_inspect_workbook" => {
            ensure_exact_arguments(arguments, &["workbook_id"])?;
            let workbook_id = required_text(arguments, "workbook_id", 96)?;
            if !normalized
                .get("running")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                return Err(excel_not_running_error(&normalized));
            }
            let workbook = normalized
                .get("workbooks")
                .and_then(Value::as_array)
                .and_then(|workbooks| {
                    workbooks.iter().find(|workbook| {
                        workbook.get("workbook_id").and_then(Value::as_str) == Some(workbook_id)
                    })
                })
                .ok_or_else(|| {
                    anyhow!(
                        "Excel workbook identity is missing or stale; list open workbooks again"
                    )
                })?;
            Ok(json!({
                "platform": normalized.get("platform"),
                "excel_running": true,
                "read_only": true,
                "safe_no_launch": true,
                "workbook": workbook,
            }))
        }
        _ => Err(anyhow!(
            "Excel Live Control operation is not implemented: {operation}"
        )),
    }
}

fn read_platform_snapshot() -> Result<Value> {
    match std::env::consts::OS {
        "macos" => read_macos_snapshot(),
        "windows" => read_windows_snapshot(),
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

fn read_platform_status() -> Result<Value> {
    match std::env::consts::OS {
        "macos" => read_macos_status(),
        "windows" => read_windows_status(),
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

fn read_platform_range(request: &Value) -> Result<Value> {
    match std::env::consts::OS {
        "macos" => run_json_command_with_stdin(
            MACOS_OSASCRIPT_PATH,
            &["-l", "JavaScript", "-e", MACOS_RANGE_READ_SCRIPT],
            request,
            "macOS Excel bounded range bridge",
        ),
        "windows" => {
            let powershell = windows_powershell_path()?;
            run_json_command_with_stdin(
                powershell.as_path(),
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    WINDOWS_RANGE_READ_SCRIPT,
                ],
                request,
                "Windows Excel bounded range bridge",
            )
        }
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

fn write_platform_range(request: &Value) -> Result<Value> {
    match std::env::consts::OS {
        "macos" => run_json_command_with_stdin(
            MACOS_OSASCRIPT_PATH,
            &["-l", "JavaScript", "-e", MACOS_RANGE_WRITE_SCRIPT],
            request,
            "macOS Excel bounded range write bridge",
        ),
        "windows" => {
            let powershell = windows_powershell_path()?;
            run_json_command_with_stdin(
                powershell.as_path(),
                &[
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-Command",
                    WINDOWS_RANGE_WRITE_SCRIPT,
                ],
                request,
                "Windows Excel bounded range write bridge",
            )
        }
        _ => Err(anyhow!(
            "Excel Live Control is currently available only on macOS and Windows"
        )),
    }
}

fn read_macos_snapshot() -> Result<Value> {
    if !macos_excel_installed() {
        return Ok(json!({
            "schema_version": 1,
            "installed": false,
            "running": false,
            "runtime_instance": null,
            "application_version": null,
            "workbooks_total": 0,
            "workbooks_truncated": false,
            "workbooks": [],
        }));
    }
    run_json_command(
        MACOS_OSASCRIPT_PATH,
        &["-l", "JavaScript", "-e", MACOS_SNAPSHOT_SCRIPT],
        "macOS Excel automation bridge",
    )
}

fn read_macos_status() -> Result<Value> {
    if !macos_excel_installed() {
        return Ok(stopped_platform_snapshot(false));
    }
    run_json_command(
        MACOS_OSASCRIPT_PATH,
        &["-l", "JavaScript", "-e", MACOS_STATUS_SCRIPT],
        "macOS Excel status bridge",
    )
}

fn read_windows_snapshot() -> Result<Value> {
    let powershell = windows_powershell_path()?;
    run_json_command(
        powershell.as_path(),
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_SNAPSHOT_SCRIPT,
        ],
        "Windows Excel automation bridge",
    )
}

fn read_windows_status() -> Result<Value> {
    let powershell = windows_powershell_path()?;
    run_json_command(
        powershell.as_path(),
        &[
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            WINDOWS_STATUS_SCRIPT,
        ],
        "Windows Excel status bridge",
    )
}

fn stopped_platform_snapshot(installed: bool) -> Value {
    json!({
        "schema_version": 1,
        "installed": installed,
        "running": false,
        "runtime_instance": null,
        "application_version": null,
        "workbooks_total": 0,
        "workbooks_truncated": false,
        "workbook_metadata_omitted": true,
        "workbooks": [],
    })
}

fn macos_excel_installed() -> bool {
    if regular_non_symlink_dir(Path::new(MACOS_EXCEL_APPLICATION_PATH)) {
        return true;
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| {
            regular_non_symlink_dir(home.join("Applications/Microsoft Excel.app").as_path())
        })
        .unwrap_or(false)
}

fn windows_powershell_path() -> Result<PathBuf> {
    let system_root = std::env::var_os("SystemRoot")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::ffi::OsString::from(r"C:\Windows"));
    let candidate = PathBuf::from(system_root)
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    if !candidate.is_absolute() || !regular_non_symlink_file(candidate.as_path()) {
        bail!("Excel Live Control requires the fixed Windows PowerShell system executable");
    }
    Ok(candidate)
}

fn regular_non_symlink_file(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_file() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn regular_non_symlink_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir() && !metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn run_json_command<P: AsRef<Path>>(program: P, args: &[&str], label: &str) -> Result<Value> {
    run_json_command_bytes(program, args, None, label)
}

fn run_json_command_with_stdin<P: AsRef<Path>>(
    program: P,
    args: &[&str],
    request: &Value,
    label: &str,
) -> Result<Value> {
    let request = serde_json::to_vec(request).context("encode private Excel bridge request")?;
    if request.len() as u64 > MAX_BRIDGE_OUTPUT_BYTES {
        bail!("private Excel bridge request exceeds the bounded input limit");
    }
    run_json_command_bytes(program, args, Some(request.as_slice()), label)
}

fn run_json_command_bytes<P: AsRef<Path>>(
    program: P,
    args: &[&str],
    stdin: Option<&[u8]>,
    label: &str,
) -> Result<Value> {
    let is_write_bridge = label.contains("range write bridge");
    let mut child = Command::new(program.as_ref())
        .args(args)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {label}"))?;
    if let Some(stdin) = stdin {
        let write_result = child
            .stdin
            .take()
            .context("open private Excel bridge stdin")?
            .write_all(stdin)
            .with_context(|| format!("write private request to {label}"));
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    }
    let stdout_reader = child.stdout.take().context("read Excel bridge stdout")?;
    let stderr_reader = child.stderr.take().context("read Excel bridge stderr")?;
    let stdout_thread =
        thread::spawn(move || read_bounded_pipe(stdout_reader, "Excel bridge stdout"));
    let stderr_thread =
        thread::spawn(move || read_bounded_pipe(stderr_reader, "Excel bridge stderr"));
    let deadline = Instant::now()
        + if is_write_bridge {
            WRITE_BRIDGE_TIMEOUT
        } else {
            BRIDGE_TIMEOUT
        };
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .with_context(|| format!("wait for {label}"))?
        {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            if is_write_bridge {
                bail!("{label} timed out; exact mutation and rollback state could not be verified, so inspect the target range before any retry");
            }
            bail!("{label} timed out without launching or closing Microsoft Excel");
        }
        thread::sleep(Duration::from_millis(20));
    };

    let stdout = stdout_thread
        .join()
        .map_err(|_| anyhow!("Excel bridge stdout reader failed"))?;
    let stderr = stderr_thread
        .join()
        .map_err(|_| anyhow!("Excel bridge stderr reader failed"))?;
    let stdout = match stdout {
        Ok(stdout) => stdout,
        Err(_) if is_write_bridge => bail!(
            "{label} returned an oversized or unreadable result; inspect the target range before any retry"
        ),
        Err(error) => return Err(error),
    };
    let stderr = match stderr {
        Ok(stderr) => stderr,
        Err(_) if is_write_bridge => bail!(
            "{label} returned oversized diagnostics; inspect the target range before any retry"
        ),
        Err(error) => return Err(error),
    };
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        if stderr.contains("-1743") || stderr.to_ascii_lowercase().contains("not authorized") {
            bail!(
                "macOS denied Microsoft Excel Automation access; allow ChatOS to control Microsoft Excel in System Settings"
            );
        }
        if is_write_bridge {
            bail!("{label} failed; exact mutation and rollback state could not be verified, so inspect the target range before any retry");
        }
        bail!("{label} failed without changing Microsoft Excel");
    }
    if !stderr.is_empty() {
        if is_write_bridge {
            bail!("{label} returned unexpected diagnostics; inspect the target range before any retry");
        }
        bail!("{label} returned unexpected diagnostic output");
    }
    match serde_json::from_slice(stdout.as_slice()) {
        Ok(value) => Ok(value),
        Err(_) if is_write_bridge => {
            bail!("{label} returned an invalid result; inspect the target range before any retry")
        }
        Err(error) => Err(error).with_context(|| format!("decode bounded {label} response")),
    }
}

fn read_bounded_pipe<R: Read>(reader: R, label: &str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_BRIDGE_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {label}"))?;
    if bytes.len() as u64 > MAX_BRIDGE_OUTPUT_BYTES {
        bail!("{label} exceeds the bounded output limit");
    }
    Ok(bytes)
}

fn normalize_snapshot(snapshot: Value) -> Result<Value> {
    let object = snapshot
        .as_object()
        .context("Excel automation response must be an object")?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        bail!("Excel automation response has an unsupported schema version");
    }
    let installed = required_bool(object, "installed")?;
    let running = required_bool(object, "running")?;
    if running && !installed {
        bail!("Excel automation response cannot be running when Excel is not installed");
    }
    let runtime_instance =
        optional_bounded_text(object.get("runtime_instance"), "runtime_instance", 128)?;
    if running && runtime_instance.is_none() {
        bail!("Excel automation response is missing the running instance identity");
    }
    let application_version = optional_bounded_text(
        object.get("application_version"),
        "application_version",
        128,
    )?;
    let workbooks_total = required_usize(object, "workbooks_total", 10_000)?;
    let workbooks_truncated = required_bool(object, "workbooks_truncated")?;
    let workbook_metadata_omitted = object
        .get("workbook_metadata_omitted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let workbooks = object
        .get("workbooks")
        .and_then(Value::as_array)
        .context("Excel automation workbooks must be an array")?;
    if workbooks.len() > MAX_OPEN_WORKBOOKS {
        bail!("Excel automation response exceeds the open workbook limit");
    }
    if !running && (!workbooks.is_empty() || workbooks_total != 0) {
        bail!("stopped Excel automation response cannot contain workbooks");
    }
    if workbook_metadata_omitted && !workbooks.is_empty() {
        bail!("Excel status response cannot include omitted workbook metadata");
    }
    if workbooks_truncated != (workbooks_total > MAX_OPEN_WORKBOOKS) {
        bail!("Excel automation workbook truncation metadata is inconsistent");
    }
    if !workbook_metadata_omitted && workbooks.len() != workbooks_total.min(MAX_OPEN_WORKBOOKS) {
        bail!("Excel automation workbook count does not match its bounded metadata");
    }

    let platform = std::env::consts::OS;
    let mut normalized_workbooks = Vec::with_capacity(workbooks.len());
    let mut workbook_ids = std::collections::BTreeSet::new();
    let mut active_workbooks = 0usize;
    for (position, workbook) in workbooks.iter().enumerate() {
        let workbook = normalize_workbook(workbook, runtime_instance.as_deref().unwrap_or(""))?;
        if workbook.get("index").and_then(Value::as_u64) != Some((position + 1) as u64) {
            bail!("Excel automation workbook indices are not exact and sequential");
        }
        let workbook_id = workbook
            .get("workbook_id")
            .and_then(Value::as_str)
            .expect("normalized workbook identity");
        if !workbook_ids.insert(workbook_id.to_string()) {
            bail!("Excel automation response contains duplicate workbook identities");
        }
        if workbook.get("active").and_then(Value::as_bool) == Some(true) {
            active_workbooks += 1;
        }
        normalized_workbooks.push(workbook);
    }
    if active_workbooks > 1 {
        bail!("Excel automation response contains more than one active workbook");
    }

    Ok(json!({
        "platform": platform,
        "installed": installed,
        "running": running,
        "application_version": application_version,
        "workbooks_total": workbooks_total,
        "workbooks_truncated": workbooks_truncated,
        "workbooks": normalized_workbooks,
    }))
}

fn normalize_workbook(workbook: &Value, runtime_instance: &str) -> Result<Value> {
    let object = workbook
        .as_object()
        .context("Excel workbook metadata must be an object")?;
    let index = required_usize(object, "index", MAX_OPEN_WORKBOOKS)?;
    if index == 0 {
        bail!("Excel workbook index must be one-based");
    }
    let name = required_bounded_text(object, "name", MAX_WORKBOOK_NAME_CHARACTERS)?;
    let identity_source =
        required_bounded_text(object, "identity_source", MAX_IDENTITY_SOURCE_CHARACTERS)?;
    let saved = required_bool(object, "saved")?;
    let read_only = required_bool(object, "read_only")?;
    let active = required_bool(object, "active")?;
    let sheet_count = required_usize(object, "sheet_count", 100_000)?;
    let sheets_truncated = required_bool(object, "sheets_truncated")?;
    let sheets = object
        .get("sheets")
        .and_then(Value::as_array)
        .context("Excel workbook sheets must be an array")?;
    if sheets.len() > MAX_WORKSHEETS_PER_WORKBOOK {
        bail!("Excel workbook exceeds the worksheet metadata limit");
    }
    if sheets_truncated != (sheet_count > MAX_WORKSHEETS_PER_WORKBOOK) {
        bail!("Excel worksheet truncation metadata is inconsistent");
    }
    if sheets.len() != sheet_count.min(MAX_WORKSHEETS_PER_WORKBOOK) {
        bail!("Excel worksheet count does not match its bounded metadata");
    }
    let workbook_id = workbook_identity(runtime_instance, index, name, identity_source);
    let mut normalized_sheets = Vec::with_capacity(sheets.len());
    let mut sheet_names = std::collections::BTreeSet::new();
    let mut active_sheets = 0usize;
    for (position, sheet) in sheets.iter().enumerate() {
        let sheet = normalize_sheet(sheet, workbook_id.as_str())?;
        if sheet.get("index").and_then(Value::as_u64) != Some((position + 1) as u64) {
            bail!("Excel worksheet indices are not exact and sequential");
        }
        let sheet_name = sheet
            .get("name")
            .and_then(Value::as_str)
            .expect("normalized worksheet name");
        if !sheet_names.insert(sheet_name.to_lowercase()) {
            bail!("Excel workbook contains duplicate worksheet names");
        }
        if sheet.get("active").and_then(Value::as_bool) == Some(true) {
            active_sheets += 1;
        }
        normalized_sheets.push(sheet);
    }
    if active_sheets > 1 || (!active && active_sheets != 0) {
        bail!("Excel workbook active worksheet metadata is inconsistent");
    }
    Ok(json!({
        "workbook_id": workbook_id,
        "name": name,
        "index": index,
        "saved": saved,
        "read_only": read_only,
        "active": active,
        "sheet_count": sheet_count,
        "sheets_truncated": sheets_truncated,
        "sheets": normalized_sheets,
    }))
}

fn normalize_sheet(sheet: &Value, workbook_id: &str) -> Result<Value> {
    let object = sheet
        .as_object()
        .context("Excel worksheet metadata must be an object")?;
    let index = required_usize(object, "index", MAX_WORKSHEETS_PER_WORKBOOK)?;
    if index == 0 {
        bail!("Excel worksheet index must be one-based");
    }
    let name = required_bounded_text(object, "name", MAX_WORKSHEET_NAME_CHARACTERS)?;
    let visible = required_bounded_text(object, "visible", 32)?;
    if !matches!(visible, "visible" | "hidden" | "very_hidden" | "unknown") {
        bail!("Excel worksheet visibility is unsupported");
    }
    let worksheet_id = worksheet_identity(workbook_id, index, name);
    Ok(json!({
        "worksheet_id": worksheet_id,
        "index": index,
        "name": name,
        "visible": visible,
        "protected": required_bool(object, "protected")?,
        "active": required_bool(object, "active")?,
    }))
}

fn status_response(snapshot: &Value) -> Value {
    let installed = snapshot
        .get("installed")
        .and_then(Value::as_bool)
        .expect("normalized Excel installed status");
    let running = snapshot
        .get("running")
        .and_then(Value::as_bool)
        .expect("normalized Excel running status");
    let status = if !installed {
        "excel_not_installed"
    } else if !running {
        "excel_not_running"
    } else {
        "ready"
    };
    json!({
        "platform": snapshot.get("platform"),
        "status": status,
        "excel_installed": installed,
        "excel_running": running,
        "application_version": snapshot.get("application_version"),
        "open_workbook_count": snapshot.get("workbooks_total"),
        "workbooks_truncated": snapshot.get("workbooks_truncated"),
        "read_only": false,
        "discovery_read_only": true,
        "safe_no_launch": true,
        "cell_content_access": true,
        "max_range_cells": MAX_RANGE_CELLS,
        "write_access": true,
        "write_requires_interactive_approval": true,
        "number_format_write_access": true,
        "number_format_presets": ["general", "integer", "decimal_2", "percent_2", "date", "datetime", "text"],
        "write_saves_workbook": false,
    })
}

fn workbook_list_projection(workbook: &Value) -> Value {
    json!({
        "workbook_id": workbook.get("workbook_id"),
        "name": workbook.get("name"),
        "index": workbook.get("index"),
        "saved": workbook.get("saved"),
        "read_only": workbook.get("read_only"),
        "active": workbook.get("active"),
        "sheet_count": workbook.get("sheet_count"),
        "sheets_truncated": workbook.get("sheets_truncated"),
    })
}

fn workbook_identity(
    runtime_instance: &str,
    index: usize,
    name: &str,
    identity_source: &str,
) -> String {
    let mut hasher = Sha256::new();
    let index = index.to_string();
    for value in [
        "chatos-excel-workbook-v1",
        std::env::consts::OS,
        runtime_instance,
        index.as_str(),
        name,
        identity_source,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("excel_wb_{}", hex::encode(hasher.finalize()))
}

fn worksheet_identity(workbook_id: &str, index: usize, name: &str) -> String {
    let mut hasher = Sha256::new();
    let index = index.to_string();
    for value in [
        "chatos-excel-worksheet-v1",
        std::env::consts::OS,
        workbook_id,
        index.as_str(),
        name,
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("excel_ws_{}", hex::encode(hasher.finalize()))
}

fn resolve_range_read_target(
    raw_snapshot: &Value,
    normalized: &Value,
    workbook_id: &str,
    worksheet_id: &str,
) -> Result<RangeReadTarget> {
    let raw = raw_snapshot
        .as_object()
        .context("Excel automation response must be an object")?;
    let runtime_instance = required_bounded_text(raw, "runtime_instance", 128)?.to_string();
    let workbook = normalized
        .get("workbooks")
        .and_then(Value::as_array)
        .and_then(|workbooks| {
            workbooks.iter().find(|workbook| {
                workbook.get("workbook_id").and_then(Value::as_str) == Some(workbook_id)
            })
        })
        .ok_or_else(|| {
            anyhow!("Excel workbook identity is missing or stale; list open workbooks again")
        })?;
    let workbook_index = workbook
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .expect("normalized Excel workbook index");
    let workbook_name = workbook
        .get("name")
        .and_then(Value::as_str)
        .expect("normalized Excel workbook name")
        .to_string();
    let raw_workbook = raw
        .get("workbooks")
        .and_then(Value::as_array)
        .and_then(|workbooks| workbooks.get(workbook_index - 1))
        .and_then(Value::as_object)
        .context("Excel private workbook identity is missing")?;
    if required_usize(raw_workbook, "index", MAX_OPEN_WORKBOOKS)? != workbook_index
        || required_bounded_text(raw_workbook, "name", MAX_WORKBOOK_NAME_CHARACTERS)?
            != workbook_name
    {
        bail!("Excel private workbook identity does not match normalized metadata");
    }
    let workbook_identity_source = required_bounded_text(
        raw_workbook,
        "identity_source",
        MAX_IDENTITY_SOURCE_CHARACTERS,
    )?
    .to_string();
    if workbook_identity(
        runtime_instance.as_str(),
        workbook_index,
        workbook_name.as_str(),
        workbook_identity_source.as_str(),
    ) != workbook_id
    {
        bail!("Excel private workbook identity is stale");
    }

    let worksheet = workbook
        .get("sheets")
        .and_then(Value::as_array)
        .and_then(|worksheets| {
            worksheets.iter().find(|worksheet| {
                worksheet.get("worksheet_id").and_then(Value::as_str) == Some(worksheet_id)
            })
        })
        .ok_or_else(|| {
            anyhow!("Excel worksheet identity is missing or stale; inspect the workbook again")
        })?;
    let worksheet_index = worksheet
        .get("index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .expect("normalized Excel worksheet index");
    let worksheet_name = worksheet
        .get("name")
        .and_then(Value::as_str)
        .expect("normalized Excel worksheet name")
        .to_string();
    if worksheet_identity(workbook_id, worksheet_index, worksheet_name.as_str()) != worksheet_id {
        bail!("Excel worksheet identity is stale");
    }

    Ok(RangeReadTarget {
        runtime_instance,
        workbook_id: workbook_id.to_string(),
        workbook_index,
        workbook_name,
        workbook_identity_source,
        workbook_read_only: workbook
            .get("read_only")
            .and_then(Value::as_bool)
            .expect("normalized Excel workbook read-only state"),
        worksheet_id: worksheet_id.to_string(),
        worksheet_index,
        worksheet_name,
        worksheet_visibility: worksheet
            .get("visible")
            .and_then(Value::as_str)
            .expect("normalized Excel worksheet visibility")
            .to_string(),
        worksheet_protected: worksheet
            .get("protected")
            .and_then(Value::as_bool)
            .expect("normalized Excel worksheet protection"),
    })
}

fn range_read_bridge_request(target: &RangeReadTarget, range: &A1Range) -> Value {
    json!({
        "schema_version": 1,
        "runtime_instance": target.runtime_instance,
        "workbook_index": target.workbook_index,
        "workbook_name": target.workbook_name,
        "workbook_identity_source": target.workbook_identity_source,
        "worksheet_index": target.worksheet_index,
        "worksheet_name": target.worksheet_name,
        "range_address": range.canonical,
        "start_row": range.start_row,
        "start_column": range.start_column,
        "row_count": range.row_count,
        "column_count": range.column_count,
        "cell_count": range.cell_count,
    })
}

fn normalize_range_read_response(
    response: Value,
    target: &RangeReadTarget,
    range: &A1Range,
) -> Result<Vec<Value>> {
    let object = response
        .as_object()
        .context("Excel range response must be an object")?;
    if object.get("schema_version").and_then(Value::as_u64) != Some(1) {
        bail!("Excel range response has an unsupported schema version");
    }
    for (field, expected) in [
        ("runtime_instance", target.runtime_instance.as_str()),
        ("workbook_name", target.workbook_name.as_str()),
        ("worksheet_name", target.worksheet_name.as_str()),
        ("range_address", range.canonical.as_str()),
    ] {
        if object.get(field).and_then(Value::as_str) != Some(expected) {
            bail!("Excel range response identity is stale or mismatched");
        }
    }
    for (field, expected) in [
        ("workbook_index", target.workbook_index),
        ("worksheet_index", target.worksheet_index),
        ("start_row", range.start_row),
        ("start_column", range.start_column),
        ("row_count", range.row_count),
        ("column_count", range.column_count),
        ("cell_count", range.cell_count),
    ] {
        if required_usize(object, field, MAX_EXCEL_ROWS.max(MAX_RANGE_CELLS))? != expected {
            bail!("Excel range response geometry is stale or mismatched");
        }
    }
    let cells = object
        .get("cells")
        .and_then(Value::as_array)
        .context("Excel range response cells must be an array")?;
    if cells.len() != range.cell_count {
        bail!("Excel range response cell count is inconsistent");
    }

    let mut normalized = Vec::with_capacity(cells.len());
    for (position, cell) in cells.iter().enumerate() {
        let cell = cell
            .as_object()
            .context("Excel range cell must be an object")?;
        let expected_row_offset = position / range.column_count;
        let expected_column_offset = position % range.column_count;
        if required_usize(cell, "row_offset", range.row_count)? != expected_row_offset
            || required_usize(cell, "column_offset", range.column_count)? != expected_column_offset
        {
            bail!("Excel range cell ordering is inconsistent");
        }
        let displayed_text = required_bounded_cell_text(cell, "displayed_text")?;
        let value = normalize_cell_scalar(cell.get("value"))?;
        let value_truncated = required_bool(cell, "value_truncated")?;
        let displayed_text_truncated = required_bool(cell, "displayed_text_truncated")?;
        let has_formula = required_bool(cell, "has_formula")?;
        let formula_truncated = required_bool(cell, "formula_truncated")?;
        let formula_hidden = required_bool(cell, "formula_hidden")?;
        let formula_external_reference = required_bool(cell, "formula_external_reference")?;
        let number_format_truncated = required_bool(cell, "number_format_truncated")?;
        let number_format_unavailable = required_bool(cell, "number_format_unavailable")?;
        let is_error = required_bool(cell, "is_error")?;
        if (formula_hidden || formula_external_reference) && !has_formula {
            bail!("Excel range response formula redaction metadata is inconsistent");
        }
        if is_error && !has_formula {
            bail!("Excel range response error metadata is inconsistent");
        }
        let formula = match cell.get("formula") {
            None | Some(Value::Null) => None,
            Some(Value::String(formula)) => {
                validate_bounded_text(formula, "formula", MAX_CELL_TEXT_CHARACTERS)?;
                if !has_formula || formula_hidden || formula_external_reference {
                    bail!("Excel range response exposed a disallowed formula");
                }
                if !formula.starts_with('=') || formula_contains_external_reference(formula) {
                    bail!("Excel range response formula is unsupported or externally linked");
                }
                Some(formula.clone())
            }
            _ => bail!("Excel range response formula must be text or null"),
        };
        if has_formula && !formula_hidden && !formula_external_reference && formula.is_none() {
            bail!("Excel range response omitted an accessible formula");
        }
        if formula.is_none() && formula_truncated {
            bail!("Excel range response formula truncation metadata is inconsistent");
        }
        let number_format = match cell.get("number_format") {
            None | Some(Value::Null) => None,
            Some(Value::String(number_format)) => {
                validate_bounded_text(
                    number_format,
                    "number format",
                    MAX_NUMBER_FORMAT_CHARACTERS,
                )?;
                Some(number_format.clone())
            }
            _ => bail!("Excel range response number format must be text or null"),
        };
        if number_format_unavailable {
            if number_format.is_some() || number_format_truncated {
                bail!("Excel range response unavailable number format metadata is inconsistent");
            }
        } else if number_format.as_deref().is_none_or(str::is_empty) {
            bail!("Excel range response omitted an available non-empty number format");
        }
        if value_truncated && !value.as_ref().is_some_and(Value::is_string) {
            bail!("Excel range response value truncation metadata is inconsistent");
        }
        let number_format_preset = number_format
            .as_deref()
            .and_then(number_format_preset_for_code);
        let number_format_available = !number_format_unavailable;
        let number_format_custom = number_format_available && number_format_preset.is_none();
        let row = range.start_row + expected_row_offset;
        let column = range.start_column + expected_column_offset;
        let status = if is_error {
            "error"
        } else if has_formula {
            "formula"
        } else if value
            .as_ref()
            .is_none_or(|value| value.is_null() || value.as_str().is_some_and(str::is_empty))
            && displayed_text.is_empty()
        {
            "blank"
        } else {
            "value"
        };
        normalized.push(json!({
            "address": format!("{}{}", excel_column_name(column), row),
            "value": value.unwrap_or(Value::Null),
            "value_truncated": value_truncated,
            "displayed_text": displayed_text,
            "displayed_text_truncated": displayed_text_truncated,
            "has_formula": has_formula,
            "formula": formula,
            "formula_truncated": formula_truncated,
            "formula_hidden": formula_hidden,
            "formula_external_reference": formula_external_reference,
            "number_format": number_format,
            "number_format_truncated": number_format_truncated,
            "number_format_unavailable": number_format_unavailable,
            "number_format_available": number_format_available,
            "number_format_exact": number_format_available && !number_format_truncated,
            "number_format_preset": number_format_preset,
            "number_format_custom": number_format_custom,
            "status": status,
        }));
    }
    Ok(normalized)
}

fn normalize_cell_scalar(value: Option<&Value>) -> Result<Option<Value>> {
    match value {
        None => bail!("Excel range response is missing a cell value"),
        Some(Value::Null) => Ok(Some(Value::Null)),
        Some(Value::Bool(value)) => Ok(Some(Value::Bool(*value))),
        Some(Value::Number(value)) => Ok(Some(Value::Number(value.clone()))),
        Some(Value::String(value)) => {
            validate_bounded_text(value, "cell value", MAX_CELL_TEXT_CHARACTERS)?;
            Ok(Some(Value::String(value.clone())))
        }
        _ => bail!("Excel range response cell value must be a bounded JSON scalar"),
    }
}

fn required_bounded_cell_text<'a>(object: &'a Map<String, Value>, field: &str) -> Result<&'a str> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("Excel range response is missing text {field}"))?;
    validate_bounded_text(value, field, MAX_CELL_TEXT_CHARACTERS)?;
    Ok(value)
}

fn range_read_response(target: &RangeReadTarget, range: &A1Range, cells: Vec<Value>) -> Value {
    let range_snapshot_id = range_snapshot_id(target, range, cells.as_slice());
    let rows = cells
        .chunks(range.column_count)
        .map(|row| Value::Array(row.iter().map(public_cell_projection).collect()))
        .collect::<Vec<_>>();
    json!({
        "platform": std::env::consts::OS,
        "excel_running": true,
        "read_only": true,
        "safe_no_launch": true,
        "range_snapshot_id": range_snapshot_id,
        "workbook": {
            "workbook_id": target.workbook_id,
            "name": target.workbook_name,
            "index": target.workbook_index,
        },
        "worksheet": {
            "worksheet_id": target.worksheet_id,
            "name": target.worksheet_name,
            "index": target.worksheet_index,
            "protected": target.worksheet_protected,
        },
        "range": {
            "address": range.canonical,
            "start_row": range.start_row,
            "start_column": range.start_column,
            "row_count": range.row_count,
            "column_count": range.column_count,
            "cell_count": range.cell_count,
        },
        "cells": rows,
    })
}

fn public_cell_projection(cell: &Value) -> Value {
    let mut object = cell
        .as_object()
        .expect("normalized Excel range cell object")
        .clone();
    object.remove("number_format");
    object.remove("number_format_truncated");
    object.remove("number_format_unavailable");
    Value::Object(object)
}

fn range_snapshot_id(target: &RangeReadTarget, range: &A1Range, cells: &[Value]) -> String {
    let mut hasher = Sha256::new();
    for value in [
        "chatos-excel-range-snapshot-v2",
        std::env::consts::OS,
        target.runtime_instance.as_str(),
        target.workbook_id.as_str(),
        target.worksheet_id.as_str(),
        range.canonical.as_str(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    let cells = serde_json::to_vec(cells).expect("normalized Excel range cells serialize");
    hasher.update((cells.len() as u64).to_be_bytes());
    hasher.update(cells);
    format!("excel_range_{}", hex::encode(hasher.finalize()))
}

fn parse_range_write_input(arguments: &Value) -> Result<RangeWriteInput> {
    ensure_exact_arguments(
        arguments,
        &[
            "workbook_id",
            "worksheet_id",
            "range",
            "expected_snapshot_id",
            "cells",
        ],
    )?;
    let workbook_id = required_text(arguments, "workbook_id", 96)?.to_string();
    let worksheet_id = required_text(arguments, "worksheet_id", 96)?.to_string();
    let range = parse_a1_range(required_text(arguments, "range", 32)?)?;
    let expected_snapshot_id = required_text(
        arguments,
        "expected_snapshot_id",
        MAX_SNAPSHOT_ID_CHARACTERS,
    )?;
    let snapshot_suffix = expected_snapshot_id
        .strip_prefix("excel_range_")
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| anyhow!("expected_snapshot_id must come from excel_read_range"))?;
    debug_assert_eq!(snapshot_suffix.len(), 64);

    let rows = arguments
        .get("cells")
        .and_then(Value::as_array)
        .context("cells must be an exact rectangular row matrix")?;
    if rows.len() != range.row_count {
        bail!("cells row count must exactly match the target range");
    }
    let mut cells = Vec::with_capacity(range.cell_count);
    for row in rows {
        let row = row.as_array().context("each cells row must be an array")?;
        if row.len() != range.column_count {
            bail!("cells column count must exactly match the target range");
        }
        for cell in row {
            cells.push(parse_write_cell(cell)?);
        }
    }
    if cells.len() != range.cell_count {
        bail!("cells count must exactly match the target range");
    }
    Ok(RangeWriteInput {
        workbook_id,
        worksheet_id,
        range,
        expected_snapshot_id: expected_snapshot_id.to_string(),
        cells,
    })
}

fn parse_range_format_input(arguments: &Value) -> Result<RangeFormatInput> {
    ensure_exact_arguments(
        arguments,
        &[
            "workbook_id",
            "worksheet_id",
            "range",
            "expected_snapshot_id",
            "preset",
        ],
    )?;
    let workbook_id = required_text(arguments, "workbook_id", 96)?.to_string();
    let worksheet_id = required_text(arguments, "worksheet_id", 96)?.to_string();
    let range = parse_a1_range(required_text(arguments, "range", 32)?)?;
    let expected_snapshot_id = required_text(
        arguments,
        "expected_snapshot_id",
        MAX_SNAPSHOT_ID_CHARACTERS,
    )?;
    expected_snapshot_id
        .strip_prefix("excel_range_")
        .filter(|value| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        .ok_or_else(|| anyhow!("expected_snapshot_id must come from excel_read_range"))?;
    let preset = required_text(arguments, "preset", 32)?;
    let number_format = number_format_code_for_preset(preset)?;
    Ok(RangeFormatInput {
        workbook_id,
        worksheet_id,
        range,
        expected_snapshot_id: expected_snapshot_id.to_string(),
        preset: preset.to_string(),
        number_format: number_format.to_string(),
    })
}

fn number_format_code_for_preset(preset: &str) -> Result<&'static str> {
    match preset {
        "general" => Ok("General"),
        "integer" => Ok("0"),
        "decimal_2" => Ok("0.00"),
        "percent_2" => Ok("0.00%"),
        "date" => Ok("yyyy-mm-dd"),
        "datetime" => Ok("yyyy-mm-dd hh:mm"),
        "text" => Ok("@"),
        _ => bail!("Excel number format preset is not allowlisted"),
    }
}

fn number_format_preset_for_code(number_format: &str) -> Option<&'static str> {
    match number_format {
        "General" => Some("general"),
        "0" => Some("integer"),
        "0.00" => Some("decimal_2"),
        "0.00%" => Some("percent_2"),
        "yyyy-mm-dd" => Some("date"),
        "yyyy-mm-dd hh:mm" => Some("datetime"),
        "@" => Some("text"),
        _ => None,
    }
}

fn parse_write_cell(value: &Value) -> Result<WriteCell> {
    let object = value
        .as_object()
        .context("each Excel write cell must be a typed object")?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .context("each Excel write cell requires a kind")?;
    match kind {
        "blank" => {
            ensure_exact_object_fields(object, &["kind"], "Excel blank write cell")?;
            Ok(WriteCell::Blank)
        }
        "value" => {
            ensure_exact_object_fields(object, &["kind", "value"], "Excel value write cell")?;
            let value = object
                .get("value")
                .context("Excel value write cell is missing value")?;
            let value = match value {
                Value::Bool(value) => Value::Bool(*value),
                Value::Number(value)
                    if value
                        .as_f64()
                        .is_some_and(|value| value.is_finite() && value.abs() <= 1.0e15) =>
                {
                    Value::Number(value.clone())
                }
                Value::String(value) => {
                    validate_safe_live_text(value, "write cell value")?;
                    Value::String(value.clone())
                }
                _ => bail!(
                    "Excel value write cell must contain a bounded boolean, number, or string"
                ),
            };
            Ok(WriteCell::Value(value))
        }
        "formula" => {
            ensure_exact_object_fields(object, &["kind", "formula"], "Excel formula write cell")?;
            let formula = object
                .get("formula")
                .and_then(Value::as_str)
                .context("Excel formula write cell is missing formula text")?;
            Ok(WriteCell::Formula(validate_live_formula(formula)?))
        }
        _ => bail!("Excel write cell kind must be blank, value, or formula"),
    }
}

fn ensure_exact_object_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<()> {
    if object.len() != allowed.len()
        || object
            .keys()
            .any(|key| !allowed.iter().any(|allowed| key == allowed))
    {
        bail!("{label} contains unknown or missing fields");
    }
    Ok(())
}

fn validate_live_formula(value: &str) -> Result<String> {
    validate_bounded_text(value, "write formula", MAX_CELL_TEXT_CHARACTERS)?;
    if value.trim() != value || !value.starts_with('=') || value.len() < 2 {
        bail!("Excel write formula must start with one equals sign and have no outer whitespace");
    }
    let expression = &value[1..];
    if !expression.is_ascii()
        || expression.bytes().any(|byte| {
            !(byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'_' | b'.'
                        | b'$'
                        | b':'
                        | b','
                        | b'+'
                        | b'-'
                        | b'*'
                        | b'/'
                        | b'%'
                        | b'^'
                        | b'<'
                        | b'>'
                        | b'='
                        | b'('
                        | b')'
                        | b'!'
                        | b'\''
                        | b' '
                ))
        })
        || formula_contains_external_reference(value)
    {
        bail!("Excel write formula contains unsupported dynamic, string, or external-link syntax");
    }
    let allowed_functions = [
        "ABS", "AND", "AVERAGE", "COUNT", "COUNTA", "IF", "MAX", "MIN", "NOT", "OR", "ROUND", "SUM",
    ];
    let bytes = expression.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if bytes[cursor] == b'\'' {
            let end = expression[cursor + 1..]
                .find('\'')
                .map(|offset| cursor + 1 + offset)
                .ok_or_else(|| {
                    anyhow!("Excel write formula contains an unterminated worksheet name")
                })?;
            validate_formula_sheet_name(&expression[cursor + 1..end])?;
            if bytes.get(end + 1) != Some(&b'!') {
                bail!("quoted Excel formula identifiers are only allowed as worksheet references");
            }
            cursor = end + 2;
            continue;
        }
        if bytes[cursor].is_ascii_alphabetic()
            || bytes[cursor] == b'_'
            || (bytes[cursor] == b'$' && bytes.get(cursor + 1).is_some_and(u8::is_ascii_alphabetic))
        {
            let start = cursor;
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric()
                    || matches!(bytes[cursor], b'_' | b'.' | b'$'))
            {
                cursor += 1;
            }
            let mut lookahead = cursor;
            while lookahead < bytes.len() && bytes[lookahead] == b' ' {
                lookahead += 1;
            }
            let identifier = &expression[start..cursor];
            if bytes.get(lookahead) == Some(&b'(') {
                let function = identifier.to_ascii_uppercase();
                if !allowed_functions.contains(&function.as_str()) {
                    bail!(
                        "Excel formula function is not in the local safety allowlist: {function}"
                    );
                }
                continue;
            }
            let is_sheet_reference = bytes.get(lookahead) == Some(&b'!');
            let plain_identifier = identifier.replace('$', "");
            let is_boolean = matches!(plain_identifier.as_str(), "TRUE" | "FALSE");
            let is_cell_reference = parse_a1_cell(plain_identifier.as_str()).is_ok();
            let exponent_digit_index = if matches!(bytes.get(lookahead), Some(b'+' | b'-')) {
                lookahead + 1
            } else {
                lookahead
            };
            let is_numeric_exponent = matches!(identifier, "E" | "e")
                && start > 0
                && bytes[start - 1].is_ascii_digit()
                && bytes
                    .get(exponent_digit_index)
                    .is_some_and(u8::is_ascii_digit);
            if is_sheet_reference {
                validate_formula_sheet_name(identifier)?;
            } else if !is_boolean && !is_cell_reference && !is_numeric_exponent {
                bail!("Excel formula named ranges are disabled; use cells, booleans, safe functions, or worksheet references");
            }
        } else {
            cursor += 1;
        }
    }
    Ok(value.to_string())
}

fn validate_safe_live_text(value: &str, field: &str) -> Result<()> {
    validate_bounded_text(value, field, MAX_CELL_TEXT_CHARACTERS)?;
    if value.is_empty() {
        bail!("empty Excel text must be written as an explicit blank cell");
    }
    if value
        .trim_start()
        .chars()
        .next()
        .is_some_and(|character| matches!(character, '=' | '+' | '-' | '@' | '\''))
    {
        bail!("Excel text that could be interpreted as a formula is disabled");
    }
    Ok(())
}

fn validate_formula_sheet_name(value: &str) -> Result<()> {
    let characters = value.chars().count();
    if characters == 0
        || characters > 31
        || value.trim().is_empty()
        || value.starts_with('\'')
        || value.ends_with('\'')
        || value.chars().any(|character| {
            character.is_control() || matches!(character, ':' | '\\' | '/' | '?' | '*' | '[' | ']')
        })
    {
        bail!("Excel formula worksheet name is invalid");
    }
    Ok(())
}

fn write_cells_bridge_value(cells: &[WriteCell]) -> Value {
    Value::Array(
        cells
            .iter()
            .map(|cell| match cell {
                WriteCell::Blank => json!({"kind": "blank"}),
                WriteCell::Value(value) => json!({"kind": "value", "value": value}),
                WriteCell::Formula(formula) => {
                    json!({"kind": "formula", "formula": formula})
                }
            })
            .collect(),
    )
}

fn write_cell_summary(cells: &[WriteCell]) -> Result<WriteCellSummary> {
    let mut blank_cells = 0usize;
    let mut value_cells = 0usize;
    let mut formula_cells = 0usize;
    let mut text_characters = 0usize;
    for cell in cells {
        match cell {
            WriteCell::Blank => blank_cells += 1,
            WriteCell::Value(Value::String(value)) => {
                value_cells += 1;
                text_characters += value.chars().count();
            }
            WriteCell::Value(_) => value_cells += 1,
            WriteCell::Formula(formula) => {
                formula_cells += 1;
                text_characters += formula.chars().count();
            }
        }
    }
    let encoded = serde_json::to_vec(&write_cells_bridge_value(cells))
        .context("encode Excel write approval summary")?;
    Ok(WriteCellSummary {
        blank_cells,
        value_cells,
        formula_cells,
        text_characters,
        content_sha256: hex::encode(Sha256::digest(encoded)),
    })
}

fn parse_a1_range(value: &str) -> Result<A1Range> {
    if value.chars().count() > 32 || value.chars().any(char::is_whitespace) {
        bail!("Excel range must be a bounded canonical A1 reference");
    }
    let mut parts = value.split(':');
    let first = parts.next().expect("A1 range has a first component");
    let second = parts.next();
    if parts.next().is_some() {
        bail!("Excel range must be one contiguous canonical A1 reference");
    }
    let (start_row, start_column) = parse_a1_cell(first)?;
    let (end_row, end_column) = match second {
        Some(second) => parse_a1_cell(second)?,
        None => (start_row, start_column),
    };
    if end_row < start_row || end_column < start_column {
        bail!("Excel range must run from its upper-left cell to its lower-right cell");
    }
    let row_count = end_row - start_row + 1;
    let column_count = end_column - start_column + 1;
    let cell_count = row_count
        .checked_mul(column_count)
        .context("Excel range cell count overflow")?;
    if cell_count > MAX_RANGE_CELLS {
        bail!("Excel range exceeds the 256-cell read limit");
    }
    let canonical = if second.is_some() {
        format!(
            "{}{}:{}{}",
            excel_column_name(start_column),
            start_row,
            excel_column_name(end_column),
            end_row
        )
    } else {
        format!("{}{}", excel_column_name(start_column), start_row)
    };
    if canonical != value {
        bail!("Excel range must use canonical uppercase A1 notation");
    }
    Ok(A1Range {
        canonical,
        start_row,
        start_column,
        end_row,
        end_column,
        row_count,
        column_count,
        cell_count,
    })
}

fn parse_a1_cell(value: &str) -> Result<(usize, usize)> {
    let split = value
        .find(|character: char| character.is_ascii_digit())
        .ok_or_else(|| anyhow!("Excel range cell is missing a row number"))?;
    let (column, row) = value.split_at(split);
    if column.is_empty()
        || column.len() > 3
        || !column.bytes().all(|byte| byte.is_ascii_uppercase())
        || row.is_empty()
        || row.starts_with('0')
        || !row.bytes().all(|byte| byte.is_ascii_digit())
    {
        bail!("Excel range cell is not canonical A1 notation");
    }
    let column = column.bytes().try_fold(0usize, |value, byte| {
        value
            .checked_mul(26)
            .and_then(|value| value.checked_add((byte - b'A' + 1) as usize))
            .context("Excel range column overflow")
    })?;
    let row = row
        .parse::<usize>()
        .context("Excel range row is not a valid integer")?;
    if column == 0 || column > MAX_EXCEL_COLUMNS || row == 0 || row > MAX_EXCEL_ROWS {
        bail!("Excel range cell is outside the worksheet grid");
    }
    Ok((row, column))
}

fn excel_column_name(mut column: usize) -> String {
    let mut bytes = Vec::new();
    while column > 0 {
        column -= 1;
        bytes.push(b'A' + (column % 26) as u8);
        column /= 26;
    }
    bytes.reverse();
    String::from_utf8(bytes).expect("Excel column names are ASCII")
}

fn formula_contains_external_reference(formula: &str) -> bool {
    let lower = formula.to_ascii_lowercase();
    if lower.contains("://") || formula.contains("\\\\") {
        return true;
    }
    if let Some(open) = lower.find('[') {
        if let Some(close) = lower[open + 1..].find(']') {
            if lower[open + close + 2..].contains('!') {
                return true;
            }
        }
    }
    formula
        .as_bytes()
        .windows(3)
        .any(|window| window[0].is_ascii_alphabetic() && window[1] == b':' && window[2] == b'\\')
}

fn excel_not_running_error(snapshot: &Value) -> anyhow::Error {
    if snapshot.get("installed").and_then(Value::as_bool) == Some(false) {
        anyhow!("Microsoft Excel desktop is not installed")
    } else {
        anyhow!(
            "Microsoft Excel is not running; Excel Live Control never launches it automatically"
        )
    }
}

fn ensure_exact_arguments(arguments: &Value, allowed: &[&str]) -> Result<()> {
    let object = arguments
        .as_object()
        .context("Excel Live Control arguments must be an object")?;
    if object.len() != allowed.len()
        || object
            .keys()
            .any(|key| !allowed.iter().any(|allowed| key == allowed))
    {
        bail!("Excel Live Control arguments contain unknown or missing fields");
    }
    Ok(())
}

fn required_text<'a>(arguments: &'a Value, field: &str, max_characters: usize) -> Result<&'a str> {
    let value = arguments
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("{field} is required"))?;
    validate_bounded_text(value, field, max_characters)?;
    Ok(value)
}

fn required_bounded_text<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    max_characters: usize,
) -> Result<&'a str> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("Excel automation response is missing {field}"))?;
    validate_bounded_text(value, field, max_characters)?;
    Ok(value)
}

fn optional_bounded_text(
    value: Option<&Value>,
    field: &str,
    max_characters: usize,
) -> Result<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if !value.is_empty() => {
            validate_bounded_text(value, field, max_characters)?;
            Ok(Some(value.clone()))
        }
        _ => bail!("Excel automation response has an invalid {field}"),
    }
}

fn validate_bounded_text(value: &str, field: &str, max_characters: usize) -> Result<()> {
    if value.chars().count() > max_characters {
        bail!("Excel automation {field} exceeds the bounded text limit");
    }
    if value.chars().any(char::is_control) {
        bail!("Excel automation {field} contains a control character");
    }
    Ok(())
}

fn required_bool(object: &Map<String, Value>, field: &str) -> Result<bool> {
    object
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| anyhow!("Excel automation response is missing boolean {field}"))
}

fn required_usize(object: &Map<String, Value>, field: &str, maximum: usize) -> Result<usize> {
    let value = object
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| anyhow!("Excel automation response is missing integer {field}"))?;
    if value > maximum {
        bail!("Excel automation {field} exceeds the bounded limit");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_snapshot() -> Value {
        json!({
            "schema_version": 1,
            "installed": true,
            "running": true,
            "runtime_instance": "4242",
            "application_version": "16.99",
            "workbooks_total": 1,
            "workbooks_truncated": false,
            "workbooks": [{
                "index": 1,
                "name": "Budget.xlsx",
                "identity_source": "/private/secret/Budget.xlsx",
                "saved": false,
                "read_only": false,
                "active": true,
                "sheet_count": 2,
                "sheets_truncated": false,
                "sheets": [
                    {"index": 1, "name": "Summary", "visible": "visible", "protected": false, "active": true},
                    {"index": 2, "name": "Inputs", "visible": "hidden", "protected": true, "active": false}
                ]
            }]
        })
    }

    fn sample_range_bridge_response(
        target: &RangeReadTarget,
        range: &A1Range,
        first_value: Value,
        second_value: Value,
        second_formula: &str,
    ) -> Value {
        let first_display = first_value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| first_value.to_string());
        let second_display = second_value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| second_value.to_string());
        json!({
            "schema_version": 1,
            "runtime_instance": target.runtime_instance,
            "workbook_index": target.workbook_index,
            "workbook_name": target.workbook_name,
            "worksheet_index": target.worksheet_index,
            "worksheet_name": target.worksheet_name,
            "range_address": range.canonical,
            "start_row": range.start_row,
            "start_column": range.start_column,
            "row_count": range.row_count,
            "column_count": range.column_count,
            "cell_count": range.cell_count,
            "cells": [
                {
                    "row_offset": 0,
                    "column_offset": 0,
                    "value": first_value,
                    "value_truncated": false,
                    "displayed_text": first_display,
                    "displayed_text_truncated": false,
                    "has_formula": false,
                    "formula": null,
                    "formula_truncated": false,
                    "formula_hidden": false,
                    "formula_external_reference": false,
                    "number_format": "0.000 \"private-budget\"",
                    "number_format_truncated": false,
                    "number_format_unavailable": false,
                    "is_error": false
                },
                {
                    "row_offset": 0,
                    "column_offset": 1,
                    "value": second_value,
                    "value_truncated": false,
                    "displayed_text": second_display,
                    "displayed_text_truncated": false,
                    "has_formula": true,
                    "formula": second_formula,
                    "formula_truncated": false,
                    "formula_hidden": false,
                    "formula_external_reference": false,
                    "number_format": "0.00",
                    "number_format_truncated": false,
                    "number_format_unavailable": false,
                    "is_error": false
                }
            ]
        })
    }

    #[test]
    fn publishes_bounded_read_only_no_launch_tools() {
        let tools = tool_definitions(false);
        assert_eq!(tools.len(), 4);
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "excel_live_status",
                "excel_list_open_workbooks",
                "excel_inspect_workbook",
                "excel_read_range"
            ]
        );
        assert!(!MACOS_SNAPSHOT_SCRIPT.contains(".activate("));
        assert!(!MACOS_SNAPSHOT_SCRIPT.contains(".open("));
        assert!(!MACOS_STATUS_SCRIPT.contains(".activate("));
        assert!(!MACOS_STATUS_SCRIPT.contains(".open("));
        assert!(!WINDOWS_SNAPSHOT_SCRIPT.contains("Workbooks.Open"));
        assert!(!WINDOWS_STATUS_SCRIPT.contains("Workbooks.Open"));
        assert!(!MACOS_RANGE_READ_SCRIPT.contains(".activate("));
        assert!(!MACOS_RANGE_READ_SCRIPT.contains(".open("));
        assert!(!MACOS_RANGE_READ_SCRIPT.contains(".save("));
        assert!(!WINDOWS_RANGE_READ_SCRIPT.contains("Workbooks.Open"));
        assert!(!WINDOWS_RANGE_READ_SCRIPT.contains(".Activate("));
        assert!(!WINDOWS_RANGE_READ_SCRIPT.contains(".Save("));
        assert!(MACOS_RANGE_READ_SCRIPT.contains("fileHandleWithStandardInput"));
        assert!(WINDOWS_RANGE_READ_SCRIPT.contains("[Console]::In.ReadToEnd()"));
        assert!(WINDOWS_SNAPSHOT_SCRIPT.contains("GetActiveObject"));
        assert!(WINDOWS_RANGE_READ_SCRIPT.contains("GetActiveObject"));

        let approved_tools = tool_definitions(true);
        assert_eq!(approved_tools.len(), 6);
        assert_eq!(
            approved_tools
                .get(4)
                .and_then(|tool| tool.get("name"))
                .and_then(Value::as_str),
            Some("excel_write_range")
        );
        assert_eq!(
            approved_tools
                .last()
                .and_then(|tool| tool.get("name"))
                .and_then(Value::as_str),
            Some("excel_set_number_format")
        );
        assert!(requires_interactive_approval("excel_write_range"));
        assert!(requires_interactive_approval("excel_set_number_format"));
        assert!(!requires_interactive_approval("excel_read_range"));
        assert!(execute("excel_write_range", &json!({}))
            .expect_err("direct Excel write execution must fail before platform access")
            .to_string()
            .contains("interactive approval"));
        assert!(execute("excel_set_number_format", &json!({}))
            .expect_err("direct Excel format execution must fail before platform access")
            .to_string()
            .contains("interactive approval"));
        assert!(!MACOS_RANGE_WRITE_SCRIPT.contains(".activate("));
        assert!(!MACOS_RANGE_WRITE_SCRIPT.contains(".open("));
        assert!(!MACOS_RANGE_WRITE_SCRIPT.contains(".save("));
        assert!(!MACOS_RANGE_WRITE_SCRIPT.contains(".select("));
        assert!(!MACOS_RANGE_WRITE_SCRIPT.contains(".calculate("));
        assert!(!WINDOWS_RANGE_WRITE_SCRIPT.contains("Workbooks.Open"));
        assert!(!WINDOWS_RANGE_WRITE_SCRIPT.contains(".Activate("));
        assert!(!WINDOWS_RANGE_WRITE_SCRIPT.contains(".Save("));
        assert!(!WINDOWS_RANGE_WRITE_SCRIPT.contains(".Select("));
        assert!(!WINDOWS_RANGE_WRITE_SCRIPT.contains(".Calculate("));
        assert!(MACOS_RANGE_WRITE_SCRIPT.contains("fileHandleWithStandardInput"));
        assert!(WINDOWS_RANGE_WRITE_SCRIPT.contains("[Console]::In.ReadToEnd()"));
        assert!(WINDOWS_RANGE_WRITE_SCRIPT.contains("GetActiveObject"));
    }

    #[test]
    fn workbook_identity_hides_paths_and_binds_the_running_instance() {
        let normalized = normalize_snapshot(sample_snapshot()).expect("normalized snapshot");
        let serialized = serde_json::to_string(&normalized).expect("serialize normalized snapshot");
        assert!(!serialized.contains("/private/secret"));
        let workbook = normalized
            .pointer("/workbooks/0")
            .expect("normalized workbook");
        let workbook_id = workbook
            .get("workbook_id")
            .and_then(Value::as_str)
            .expect("workbook identity");
        assert!(workbook_id.starts_with("excel_wb_"));

        let mut restarted = sample_snapshot();
        restarted["runtime_instance"] = json!("4243");
        let restarted_id = normalize_snapshot(restarted)
            .expect("restarted snapshot")
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("restarted workbook identity")
            .to_string();
        assert_ne!(workbook_id, restarted_id);
    }

    #[test]
    fn exact_workbook_inspection_rejects_stale_identity() {
        let normalized = normalize_snapshot(sample_snapshot()).expect("normalized snapshot");
        let workbook_id = normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("workbook identity")
            .to_string();
        let inspected = execute_with_snapshot(
            "excel_inspect_workbook",
            &json!({"workbook_id": workbook_id}),
            sample_snapshot(),
        )
        .expect("inspect workbook");
        assert_eq!(
            inspected.pointer("/workbook/name").and_then(Value::as_str),
            Some("Budget.xlsx")
        );
        assert_eq!(
            inspected
                .pointer("/workbook/sheets/1/protected")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(inspected
            .pointer("/workbook/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("excel_ws_")));

        let error = execute_with_snapshot(
            "excel_inspect_workbook",
            &json!({"workbook_id": "excel_wb_stale"}),
            sample_snapshot(),
        )
        .expect_err("stale identity must fail");
        assert!(error.to_string().contains("missing or stale"));
    }

    #[test]
    fn stopped_excel_status_is_safe_and_other_operations_fail_closed() {
        let stopped = json!({
            "schema_version": 1,
            "installed": true,
            "running": false,
            "runtime_instance": null,
            "application_version": null,
            "workbooks_total": 0,
            "workbooks_truncated": false,
            "workbooks": []
        });
        let status = execute_with_snapshot("excel_live_status", &json!({}), stopped.clone())
            .expect("stopped Excel status");
        assert_eq!(
            status.get("status").and_then(Value::as_str),
            Some("excel_not_running")
        );
        assert_eq!(
            status.get("safe_no_launch").and_then(Value::as_bool),
            Some(true)
        );
        assert!(
            execute_with_snapshot("excel_list_open_workbooks", &json!({}), stopped)
                .expect_err("list requires running Excel")
                .to_string()
                .contains("never launches")
        );
    }

    #[test]
    fn status_can_report_counts_without_collecting_workbook_names() {
        let status_only = json!({
            "schema_version": 1,
            "installed": true,
            "running": true,
            "runtime_instance": "4242",
            "application_version": "16.99",
            "workbooks_total": 3,
            "workbooks_truncated": false,
            "workbook_metadata_omitted": true,
            "workbooks": []
        });
        let status = execute_with_snapshot("excel_live_status", &json!({}), status_only)
            .expect("status-only Excel snapshot");
        assert_eq!(
            status.get("open_workbook_count").and_then(Value::as_u64),
            Some(3)
        );
        assert!(!serde_json::to_string(&status)
            .expect("status JSON")
            .contains("Budget.xlsx"));
    }

    #[test]
    fn malformed_or_ambiguous_snapshots_fail_closed() {
        let mut duplicate_active = sample_snapshot();
        let second = duplicate_active["workbooks"][0].clone();
        duplicate_active["workbooks_total"] = json!(2);
        duplicate_active["workbooks"]
            .as_array_mut()
            .expect("workbooks")
            .push(second);
        assert!(normalize_snapshot(duplicate_active).is_err());

        let mut leaked_control = sample_snapshot();
        leaked_control["workbooks"][0]["name"] = json!("Budget\n.xlsx");
        assert!(normalize_snapshot(leaked_control).is_err());

        let mut missing_sheet = sample_snapshot();
        missing_sheet["workbooks"][0]["sheets"]
            .as_array_mut()
            .expect("sheets")
            .pop();
        assert!(normalize_snapshot(missing_sheet).is_err());
    }

    #[test]
    fn canonical_a1_ranges_are_bounded_to_the_excel_grid_and_256_cells() {
        let range = parse_a1_range("A1:P16").expect("256-cell range");
        assert_eq!(range.cell_count, 256);
        assert_eq!(range.start_column, 1);
        assert_eq!(range.end_column, 16);
        assert_eq!(excel_column_name(MAX_EXCEL_COLUMNS), "XFD");

        for invalid in [
            "a1",
            "$A$1",
            "Sheet1!A1",
            "A0",
            "XFE1",
            "A1048577",
            "B2:A1",
            "A1:A257",
            "A1,B2",
            "A1:B2:C3",
        ] {
            assert!(parse_a1_range(invalid).is_err(), "must reject {invalid}");
        }
    }

    #[test]
    fn write_inputs_are_exact_typed_bounded_and_formula_allowlisted() {
        let snapshot_id = format!("excel_range_{}", "a".repeat(64));
        let arguments = json!({
            "workbook_id": "excel_wb_current",
            "worksheet_id": "excel_ws_current",
            "range": "A1:B2",
            "expected_snapshot_id": snapshot_id,
            "cells": [
                [{"kind":"blank"},{"kind":"value","value":42.5}],
                [{"kind":"value","value":"Quarter 1"},{"kind":"formula","formula":"=SUM(A1:A2)"}]
            ]
        });
        let parsed = parse_range_write_input(&arguments).expect("safe write input");
        assert_eq!(parsed.range.cell_count, 4);
        assert_eq!(parsed.cells.len(), 4);
        assert_eq!(
            validate_live_formula("=ROUND(SUM(A1:A2),2)").expect("safe formula"),
            "=ROUND(SUM(A1:A2),2)"
        );
        for formula in [
            "=WEBSERVICE(A1)",
            "=RTD(A1)",
            "='[Book.xlsx]Sheet1'!A1",
            "=HYPERLINK(A1)",
            "=SUM(Table1[Amount])",
            "=\"secret\"",
        ] {
            assert!(validate_live_formula(formula).is_err(), "reject {formula}");
        }

        let mut dangerous_text = arguments.clone();
        dangerous_text["cells"][1][0] = json!({"kind":"value","value":"=CMD()"});
        assert!(parse_range_write_input(&dangerous_text).is_err());

        let mut wrong_shape = arguments.clone();
        wrong_shape["cells"][1] = json!([{"kind":"blank"}]);
        assert!(parse_range_write_input(&wrong_shape).is_err());

        let mut bad_snapshot = arguments.clone();
        bad_snapshot["expected_snapshot_id"] = json!("excel_range_stale");
        assert!(parse_range_write_input(&bad_snapshot).is_err());
    }

    #[test]
    fn write_approval_arguments_bind_content_without_exposing_cell_text() {
        let arguments = json!({
            "workbook_id": "excel_wb_current",
            "worksheet_id": "excel_ws_current",
            "range": "A1:B1",
            "expected_snapshot_id": format!("excel_range_{}", "b".repeat(64)),
            "cells": [[
                {"kind":"value","value":"private budget note"},
                {"kind":"formula","formula":"=A1"}
            ]]
        });
        let (command, args) = approval_command("excel_write_range", &arguments)
            .expect("Excel write approval command");
        assert_eq!(command, "chatos-excel-live");
        let serialized = args.join("\n");
        assert!(serialized.contains("range=A1:B1"));
        assert!(serialized.contains("cell_count=2"));
        assert!(serialized.contains("content_sha256="));
        assert!(!serialized.contains("private budget note"));
        assert!(!serialized.contains("formula==A1"));
    }

    #[test]
    fn number_format_inputs_and_approval_are_exact_and_allowlisted() {
        let arguments = json!({
            "workbook_id": "excel_wb_current",
            "worksheet_id": "excel_ws_current",
            "range": "A1:B2",
            "expected_snapshot_id": format!("excel_range_{}", "c".repeat(64)),
            "preset": "percent_2"
        });
        let parsed = parse_range_format_input(&arguments).expect("safe number format input");
        assert_eq!(parsed.range.cell_count, 4);
        assert_eq!(parsed.number_format, "0.00%");
        assert_eq!(number_format_preset_for_code("0.00%"), Some("percent_2"));
        let (command, args) = approval_command("excel_set_number_format", &arguments)
            .expect("Excel number format approval command");
        assert_eq!(command, "chatos-excel-live");
        assert!(args.iter().any(|value| value == "set_number_format"));
        assert!(args.iter().any(|value| value == "preset=percent_2"));
        assert!(args.iter().any(|value| value == "cell_count=4"));

        let mut arbitrary = arguments.clone();
        arbitrary["preset"] = json!("$#,##0.00");
        assert!(parse_range_format_input(&arbitrary).is_err());
        let mut extra = arguments.clone();
        extra["custom_format"] = json!("secret");
        assert!(parse_range_format_input(&extra).is_err());
    }

    #[test]
    fn range_target_uses_exact_opaque_workbook_and_worksheet_identities() {
        let raw = sample_snapshot();
        let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
        let workbook_id = normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("workbook ID");
        let worksheet_id = normalized
            .pointer("/workbooks/0/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .expect("worksheet ID");
        let target = resolve_range_read_target(&raw, &normalized, workbook_id, worksheet_id)
            .expect("exact range target");
        assert_eq!(target.workbook_name, "Budget.xlsx");
        assert_eq!(target.worksheet_name, "Summary");
        assert_eq!(
            target.workbook_identity_source,
            "/private/secret/Budget.xlsx"
        );

        assert!(
            resolve_range_read_target(&raw, &normalized, "excel_wb_stale", worksheet_id).is_err()
        );
        assert!(
            resolve_range_read_target(&raw, &normalized, workbook_id, "excel_ws_stale").is_err()
        );
    }

    #[test]
    fn range_response_is_typed_bounded_and_strips_private_identity() {
        let raw = sample_snapshot();
        let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
        let workbook_id = normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("workbook ID");
        let worksheet_id = normalized
            .pointer("/workbooks/0/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .expect("worksheet ID");
        let target = resolve_range_read_target(&raw, &normalized, workbook_id, worksheet_id)
            .expect("exact range target");
        let range = parse_a1_range("A1:B1").expect("range");
        let response = json!({
            "schema_version": 1,
            "runtime_instance": target.runtime_instance,
            "workbook_index": target.workbook_index,
            "workbook_name": target.workbook_name,
            "worksheet_index": target.worksheet_index,
            "worksheet_name": target.worksheet_name,
            "range_address": "A1:B1",
            "start_row": 1,
            "start_column": 1,
            "row_count": 1,
            "column_count": 2,
            "cell_count": 2,
            "cells": [
                {
                    "row_offset": 0,
                    "column_offset": 0,
                    "value": 42.5,
                    "value_truncated": false,
                    "displayed_text": "42.50",
                    "displayed_text_truncated": false,
                    "has_formula": false,
                    "formula": null,
                    "formula_truncated": false,
                    "formula_hidden": false,
                    "formula_external_reference": false,
                    "number_format": "0.000 \"private-budget\"",
                    "number_format_truncated": false,
                    "number_format_unavailable": false,
                    "is_error": false
                },
                {
                    "row_offset": 0,
                    "column_offset": 1,
                    "value": 85.0,
                    "value_truncated": false,
                    "displayed_text": "85",
                    "displayed_text_truncated": false,
                    "has_formula": true,
                    "formula": "=A1*2",
                    "formula_truncated": false,
                    "formula_hidden": false,
                    "formula_external_reference": false,
                    "number_format": "0.00",
                    "number_format_truncated": false,
                    "number_format_unavailable": false,
                    "is_error": false
                }
            ]
        });
        let cells = normalize_range_read_response(response, &target, &range)
            .expect("normalized range cells");
        let snapshot_id = range_snapshot_id(&target, &range, cells.as_slice());
        let mut reformatted = cells.clone();
        reformatted[0]["number_format"] = json!("0");
        reformatted[0]["number_format_preset"] = json!("integer");
        reformatted[0]["number_format_custom"] = json!(false);
        assert_ne!(
            snapshot_id,
            range_snapshot_id(&target, &range, reformatted.as_slice())
        );
        let projected = range_read_response(&target, &range, cells);
        assert_eq!(
            projected
                .pointer("/cells/0/1/formula")
                .and_then(Value::as_str),
            Some("=A1*2")
        );
        assert_eq!(
            projected
                .pointer("/cells/0/1/address")
                .and_then(Value::as_str),
            Some("B1")
        );
        assert_eq!(
            projected
                .pointer("/cells/0/1/number_format_preset")
                .and_then(Value::as_str),
            Some("decimal_2")
        );
        assert_eq!(
            projected
                .pointer("/cells/0/0/number_format_custom")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(projected.pointer("/cells/0/1/number_format").is_none());
        let serialized = serde_json::to_string(&projected).expect("serialized tool response");
        assert!(!serialized.contains("/private/secret"));
        assert!(!serialized.contains("identity_source"));
        assert!(!serialized.contains("private-budget"));
        assert!(projected
            .get("range_snapshot_id")
            .and_then(Value::as_str)
            .is_some_and(|value| value.starts_with("excel_range_") && value.len() == 76));
    }

    #[test]
    fn write_result_requires_exact_snapshot_safe_cells_and_verified_values() {
        let raw = sample_snapshot();
        let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
        let workbook_id = normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("workbook ID");
        let worksheet_id = normalized
            .pointer("/workbooks/0/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .expect("worksheet ID");
        let target = resolve_range_read_target(&raw, &normalized, workbook_id, worksheet_id)
            .expect("exact range target");
        ensure_write_target_is_mutable(&target).expect("mutable exact target");
        let range = parse_a1_range("A1:B1").expect("range");
        let current = normalize_range_read_response(
            sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2"),
            &target,
            &range,
        )
        .expect("current cells");
        ensure_snapshot_cells_are_write_safe(current.as_slice()).expect("safe rollback snapshot");
        let snapshot_id = range_snapshot_id(&target, &range, current.as_slice());
        let input = RangeWriteInput {
            workbook_id: workbook_id.to_string(),
            worksheet_id: worksheet_id.to_string(),
            range: range.clone(),
            expected_snapshot_id: snapshot_id,
            cells: vec![
                WriteCell::Value(json!(43.0)),
                WriteCell::Formula("=A1*3".to_string()),
            ],
        };

        let mut written =
            sample_range_bridge_response(&target, &range, json!(43.0), json!(129.0), "=A1*3");
        written["write_status"] = json!("written");
        let normalized_written =
            normalize_range_write_response(written, &target, &input, current.as_slice())
                .expect("verified write result");
        assert!(
            desired_cells_match(input.cells.as_slice(), normalized_written.as_slice())
                .expect("desired write comparison")
        );

        let mut rolled_back =
            sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2");
        rolled_back["write_status"] = json!("rolled_back");
        assert!(
            normalize_range_write_response(rolled_back, &target, &input, current.as_slice(),)
                .expect_err("rolled-back write must not report success")
                .to_string()
                .contains("restored and verified")
        );
    }

    #[test]
    fn number_format_result_preserves_contents_and_verifies_exact_preset() {
        let raw = sample_snapshot();
        let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
        let workbook_id = normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("workbook ID");
        let worksheet_id = normalized
            .pointer("/workbooks/0/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .expect("worksheet ID");
        let target = resolve_range_read_target(&raw, &normalized, workbook_id, worksheet_id)
            .expect("exact range target");
        let range = parse_a1_range("A1:B1").expect("range");
        let current = normalize_range_read_response(
            sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2"),
            &target,
            &range,
        )
        .expect("current cells");
        ensure_snapshot_cells_are_format_safe(current.as_slice()).expect("safe format snapshot");
        let input = RangeFormatInput {
            workbook_id: workbook_id.to_string(),
            worksheet_id: worksheet_id.to_string(),
            range: range.clone(),
            expected_snapshot_id: range_snapshot_id(&target, &range, current.as_slice()),
            preset: "percent_2".to_string(),
            number_format: "0.00%".to_string(),
        };
        let mut formatted =
            sample_range_bridge_response(&target, &range, json!(42.5), json!(85.0), "=A1*2");
        formatted["write_status"] = json!("formatted");
        formatted["cells"][0]["displayed_text"] = json!("4250.00%");
        formatted["cells"][1]["displayed_text"] = json!("8500.00%");
        formatted["cells"][0]["number_format"] = json!("0.00%");
        formatted["cells"][1]["number_format"] = json!("0.00%");
        let normalized_formatted =
            normalize_range_format_response(formatted, &target, &input, current.as_slice())
                .expect("verified number format result");
        assert!(formatted_cells_match(
            current.as_slice(),
            normalized_formatted.as_slice(),
            "0.00%"
        )
        .expect("formatted comparison"));

        let result = range_format_response(&target, &input, normalized_formatted, false);
        assert_eq!(
            result.get("number_format_preset").and_then(Value::as_str),
            Some("percent_2")
        );
        assert_eq!(
            result
                .pointer("/cells/0/0/number_format_preset")
                .and_then(Value::as_str),
            Some("percent_2")
        );
        assert!(result.pointer("/cells/0/0/number_format").is_none());
    }

    #[test]
    fn write_safety_rejects_read_only_hidden_protected_and_ambiguous_snapshots() {
        let raw = sample_snapshot();
        let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
        let workbook_id = normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("workbook ID");
        let visible_id = normalized
            .pointer("/workbooks/0/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .expect("visible worksheet ID");
        let protected_id = normalized
            .pointer("/workbooks/0/sheets/1/worksheet_id")
            .and_then(Value::as_str)
            .expect("protected worksheet ID");
        let visible = resolve_range_read_target(&raw, &normalized, workbook_id, visible_id)
            .expect("visible target");
        let protected = resolve_range_read_target(&raw, &normalized, workbook_id, protected_id)
            .expect("protected target");
        assert!(ensure_write_target_is_mutable(&visible).is_ok());
        assert!(ensure_write_target_is_mutable(&protected).is_err());

        let mut read_only_raw = sample_snapshot();
        read_only_raw["workbooks"][0]["read_only"] = json!(true);
        let read_only_normalized =
            normalize_snapshot(read_only_raw.clone()).expect("read-only normalized snapshot");
        let read_only_target = resolve_range_read_target(
            &read_only_raw,
            &read_only_normalized,
            read_only_normalized
                .pointer("/workbooks/0/workbook_id")
                .and_then(Value::as_str)
                .expect("read-only workbook ID"),
            read_only_normalized
                .pointer("/workbooks/0/sheets/0/worksheet_id")
                .and_then(Value::as_str)
                .expect("read-only worksheet ID"),
        )
        .expect("read-only target");
        assert!(ensure_write_target_is_mutable(&read_only_target).is_err());

        let mut hidden_raw = sample_snapshot();
        hidden_raw["workbooks"][0]["sheets"][1]["protected"] = json!(false);
        let hidden_normalized =
            normalize_snapshot(hidden_raw.clone()).expect("hidden normalized snapshot");
        let hidden_target = resolve_range_read_target(
            &hidden_raw,
            &hidden_normalized,
            hidden_normalized
                .pointer("/workbooks/0/workbook_id")
                .and_then(Value::as_str)
                .expect("hidden workbook ID"),
            hidden_normalized
                .pointer("/workbooks/0/sheets/1/worksheet_id")
                .and_then(Value::as_str)
                .expect("hidden worksheet ID"),
        )
        .expect("hidden target");
        assert!(ensure_write_target_is_mutable(&hidden_target).is_err());

        let range = parse_a1_range("A1:B1").expect("range");
        let mut response =
            sample_range_bridge_response(&visible, &range, json!(42.5), json!(85.0), "=A1*2");
        response["cells"][0]["displayed_text_truncated"] = json!(true);
        let cells = normalize_range_read_response(response, &visible, &range)
            .expect("normalized ambiguous cells");
        assert!(ensure_snapshot_cells_are_write_safe(cells.as_slice()).is_err());

        let mut unsafe_format =
            sample_range_bridge_response(&visible, &range, json!(42.5), json!(85.0), "=A1*2");
        unsafe_format["cells"][0]["number_format_truncated"] = json!(true);
        let cells = normalize_range_read_response(unsafe_format, &visible, &range)
            .expect("normalized unsafe format cells");
        assert!(ensure_snapshot_cells_are_format_safe(cells.as_slice()).is_err());
    }

    #[test]
    fn range_response_rejects_exposed_external_or_hidden_formulas() {
        assert!(formula_contains_external_reference(
            "='C:\\secret\\[Budget.xlsx]Sheet1'!A1"
        ));
        assert!(formula_contains_external_reference(
            "='https://example.test/[Budget.xlsx]Sheet1'!A1"
        ));
        assert!(formula_contains_external_reference("='[Book1]Sheet1'!A1"));
        assert!(!formula_contains_external_reference("=SUM(Table1[Amount])"));

        let raw = sample_snapshot();
        let normalized = normalize_snapshot(raw.clone()).expect("normalized snapshot");
        let workbook_id = normalized
            .pointer("/workbooks/0/workbook_id")
            .and_then(Value::as_str)
            .expect("workbook ID");
        let worksheet_id = normalized
            .pointer("/workbooks/0/sheets/0/worksheet_id")
            .and_then(Value::as_str)
            .expect("worksheet ID");
        let target = resolve_range_read_target(&raw, &normalized, workbook_id, worksheet_id)
            .expect("exact range target");
        let range = parse_a1_range("A1").expect("range");
        let exposed = json!({
            "schema_version": 1,
            "runtime_instance": target.runtime_instance,
            "workbook_index": target.workbook_index,
            "workbook_name": target.workbook_name,
            "worksheet_index": target.worksheet_index,
            "worksheet_name": target.worksheet_name,
            "range_address": "A1",
            "start_row": 1,
            "start_column": 1,
            "row_count": 1,
            "column_count": 1,
            "cell_count": 1,
            "cells": [{
                "row_offset": 0,
                "column_offset": 0,
                "value": 1,
                "value_truncated": false,
                "displayed_text": "1",
                "displayed_text_truncated": false,
                "has_formula": true,
                "formula": "='C:\\secret\\[Budget.xlsx]Sheet1'!A1",
                "formula_truncated": false,
                "formula_hidden": false,
                "formula_external_reference": false,
                "number_format": "General",
                "number_format_truncated": false,
                "number_format_unavailable": false,
                "is_error": false
            }]
        });
        assert!(normalize_range_read_response(exposed, &target, &range).is_err());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn embedded_excel_jxa_bridges_compile_without_launching_excel() {
        let temp = tempfile::tempdir().expect("temporary Excel script directory");
        for (index, script) in [
            MACOS_STATUS_SCRIPT,
            MACOS_SNAPSHOT_SCRIPT,
            MACOS_RANGE_READ_SCRIPT,
            MACOS_RANGE_WRITE_SCRIPT,
        ]
        .into_iter()
        .enumerate()
        {
            let output_path = temp.path().join(format!("excel-bridge-{index}.scpt"));
            let output = Command::new("/usr/bin/osacompile")
                .args(["-l", "JavaScript", "-e", script, "-o"])
                .arg(output_path.as_os_str())
                .output()
                .expect("compile embedded Excel JXA bridge");
            assert!(
                output.status.success(),
                "Excel JXA compilation failed: {}",
                String::from_utf8_lossy(output.stderr.as_slice())
            );
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires the local macOS Excel installation; the probe never launches Excel"]
    fn macos_status_probe_uses_the_real_no_launch_bridge() {
        let status = execute("excel_live_status", &json!({})).expect("live Excel status");
        assert_eq!(
            status.get("safe_no_launch").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            status.get("cell_content_access").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            status.get("write_access").and_then(Value::as_bool),
            Some(true)
        );
        if status.get("excel_installed").and_then(Value::as_bool) == Some(true)
            && status.get("excel_running").and_then(Value::as_bool) == Some(false)
        {
            let error = execute("excel_list_open_workbooks", &json!({}))
                .expect_err("stopped Excel cannot list workbooks");
            assert!(error.to_string().contains("never launches"));
        }
    }
}
