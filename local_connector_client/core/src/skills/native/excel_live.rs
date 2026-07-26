// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::io::Read;
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

const MACOS_OSASCRIPT_PATH: &str = "/usr/bin/osascript";
const MACOS_EXCEL_APPLICATION_PATH: &str = "/Applications/Microsoft Excel.app";

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
    let snapshot = if operation == "excel_live_status" {
        read_platform_status()?
    } else {
        read_platform_snapshot()?
    };
    execute_with_snapshot(operation, arguments, snapshot)
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
    let mut child = Command::new(program.as_ref())
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("start {label}"))?;
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
    let mut normalized_sheets = Vec::with_capacity(sheets.len());
    let mut sheet_names = std::collections::BTreeSet::new();
    let mut active_sheets = 0usize;
    for (position, sheet) in sheets.iter().enumerate() {
        let sheet = normalize_sheet(sheet)?;
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
    let workbook_id = workbook_identity(runtime_instance, index, name, identity_source);
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

fn normalize_sheet(sheet: &Value) -> Result<Value> {
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
    Ok(json!({
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
        "cell_content_access": false,
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
    fn publishes_only_read_only_no_launch_tools() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 3);
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "excel_live_status",
                "excel_list_open_workbooks",
                "excel_inspect_workbook"
            ]
        );
        assert!(!MACOS_SNAPSHOT_SCRIPT.contains(".activate("));
        assert!(!MACOS_SNAPSHOT_SCRIPT.contains(".open("));
        assert!(!MACOS_STATUS_SCRIPT.contains(".activate("));
        assert!(!MACOS_STATUS_SCRIPT.contains(".open("));
        assert!(!WINDOWS_SNAPSHOT_SCRIPT.contains("Workbooks.Open"));
        assert!(!WINDOWS_STATUS_SCRIPT.contains("Workbooks.Open"));
        assert!(WINDOWS_SNAPSHOT_SCRIPT.contains("GetActiveObject"));
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
            Some(false)
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
