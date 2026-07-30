// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

const WINDOWS_RANGE_WRITE_SCRIPT: &str = r#"
# __CHATOS_WINDOWS_PREAMBLE__
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
  # __CHATOS_WINDOWS_CELL_STATE__
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

pub(super) fn windows_range_write_script() -> String {
    super::script_fragments::expand_script(WINDOWS_RANGE_WRITE_SCRIPT)
}
