// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

pub(super) const MACOS_STATUS_SCRIPT: &str = r#"
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

pub(super) const MACOS_SNAPSHOT_SCRIPT: &str = r#"
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

pub(super) const WINDOWS_SNAPSHOT_SCRIPT: &str = r#"
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

pub(super) const WINDOWS_STATUS_SCRIPT: &str = r#"
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
