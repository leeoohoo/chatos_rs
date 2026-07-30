// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const MACOS_CELL_STATE_MARKER: &str = "/*__CHATOS_MACOS_CELL_STATE__*/";
const WINDOWS_PREAMBLE_MARKER: &str = "# __CHATOS_WINDOWS_PREAMBLE__";
const WINDOWS_CELL_STATE_MARKER: &str = "# __CHATOS_WINDOWS_CELL_STATE__";

const MACOS_CELL_STATE_FRAGMENT: &str = r#"
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
"#;

const WINDOWS_PREAMBLE_FRAGMENT: &str = r#"
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
"#;

const WINDOWS_CELL_STATE_FRAGMENT: &str = r#"
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
"#;

pub(super) fn expand_script(template: &str) -> String {
    template
        .replace(MACOS_CELL_STATE_MARKER, MACOS_CELL_STATE_FRAGMENT)
        .replace(WINDOWS_PREAMBLE_MARKER, WINDOWS_PREAMBLE_FRAGMENT)
        .replace(WINDOWS_CELL_STATE_MARKER, WINDOWS_CELL_STATE_FRAGMENT)
}
