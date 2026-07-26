// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const MAX_BRIDGE_OUTPUT_BYTES: u64 = 512 * 1024;
const BRIDGE_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_OPEN_WORKBOOKS: usize = 32;
const MAX_WORKSHEETS_PER_WORKBOOK: usize = 64;
const MAX_WORKBOOK_NAME_CHARACTERS: usize = 512;
const MAX_WORKSHEET_NAME_CHARACTERS: usize = 64;
const MAX_IDENTITY_SOURCE_CHARACTERS: usize = 4096;
const MAX_RANGE_CELLS: usize = 256;
const MAX_CELL_TEXT_CHARACTERS: usize = 128;
const MAX_EXCEL_ROWS: usize = 1_048_576;
const MAX_EXCEL_COLUMNS: usize = 16_384;

const MACOS_OSASCRIPT_PATH: &str = "/usr/bin/osascript";
const MACOS_EXCEL_APPLICATION_PATH: &str = "/Applications/Microsoft Excel.app";

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
    worksheet_id: String,
    worksheet_index: usize,
    worksheet_name: String,
    worksheet_protected: bool,
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

pub(super) fn tool_definitions() -> Vec<Value> {
    vec![
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
            "description": "Read up to 256 cells from one exact worksheet and canonical A1 range in an already-open Microsoft Excel workbook. Returns bounded scalar values, displayed text, and non-hidden non-external formulas without activating, recalculating, or mutating Excel.",
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
    ]
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
    let deadline = Instant::now() + BRIDGE_TIMEOUT;
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
            bail!("{label} timed out without launching or closing Microsoft Excel");
        }
        thread::sleep(Duration::from_millis(20));
    };

    let stdout = stdout_thread
        .join()
        .map_err(|_| anyhow!("Excel bridge stdout reader failed"))??;
    let stderr = stderr_thread
        .join()
        .map_err(|_| anyhow!("Excel bridge stderr reader failed"))??;
    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        if stderr.contains("-1743") || stderr.to_ascii_lowercase().contains("not authorized") {
            bail!(
                "macOS denied Microsoft Excel Automation access; allow ChatOS to control Microsoft Excel in System Settings"
            );
        }
        bail!("{label} failed without changing Microsoft Excel");
    }
    if !stderr.is_empty() {
        bail!("{label} returned unexpected diagnostic output");
    }
    serde_json::from_slice(stdout.as_slice())
        .with_context(|| format!("decode bounded {label} response"))
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
        "ready_read_only"
    };
    json!({
        "platform": snapshot.get("platform"),
        "status": status,
        "excel_installed": installed,
        "excel_running": running,
        "application_version": snapshot.get("application_version"),
        "open_workbook_count": snapshot.get("workbooks_total"),
        "workbooks_truncated": snapshot.get("workbooks_truncated"),
        "read_only": true,
        "safe_no_launch": true,
        "cell_content_access": true,
        "max_range_cells": MAX_RANGE_CELLS,
        "write_access": false,
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
        worksheet_id: worksheet_id.to_string(),
        worksheet_index,
        worksheet_name,
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
        if value_truncated && !value.as_ref().is_some_and(Value::is_string) {
            bail!("Excel range response value truncation metadata is inconsistent");
        }
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
    let rows = cells
        .chunks(range.column_count)
        .map(|row| Value::Array(row.to_vec()))
        .collect::<Vec<_>>();
    json!({
        "platform": std::env::consts::OS,
        "excel_running": true,
        "read_only": true,
        "safe_no_launch": true,
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

    #[test]
    fn publishes_bounded_read_only_no_launch_tools() {
        let tools = tool_definitions();
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
                    "is_error": false
                }
            ]
        });
        let cells = normalize_range_read_response(response, &target, &range)
            .expect("normalized range cells");
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
        let serialized = serde_json::to_string(&projected).expect("serialized tool response");
        assert!(!serialized.contains("/private/secret"));
        assert!(!serialized.contains("identity_source"));
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
            Some(false)
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
