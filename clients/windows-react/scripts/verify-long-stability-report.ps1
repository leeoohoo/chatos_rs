[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ReportPath,

    [ValidateRange(30, 86400)]
    [int]$ExpectedDurationSeconds = 28800,

    [string]$ExpectedWindowTitle = "ChatOS",

    [ValidateRange(0, 10)]
    [int]$AllowedSampleCountDrift = 1
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Assert-ReportCondition {
    param(
        [Parameter(Mandatory = $true)][bool]$Condition,
        [Parameter(Mandatory = $true)][string]$Message
    )

    if (-not $Condition) {
        throw "Long stability report verification failed: $Message"
    }
}

$resolvedReport = (Resolve-Path -LiteralPath $ReportPath -ErrorAction Stop).Path
$report = Get-Content -LiteralPath $resolvedReport -Raw | ConvertFrom-Json

Assert-ReportCondition ($report.schemaVersion -eq 1) "schemaVersion must be 1."
Assert-ReportCondition ($report.status -eq "passed") "status must be passed, got '$($report.status)'."
Assert-ReportCondition ($null -eq $report.error -or [string]::IsNullOrWhiteSpace([string]$report.error)) "error must be empty."
Assert-ReportCondition ($report.requestedDurationSeconds -eq $ExpectedDurationSeconds) "requestedDurationSeconds must be $ExpectedDurationSeconds."
Assert-ReportCondition ($report.sampleIntervalSeconds -ge 1 -and $report.sampleIntervalSeconds -le 300) "sampleIntervalSeconds is outside the supported range."
Assert-ReportCondition ($report.expectedWindowTitle -eq $ExpectedWindowTitle) "expectedWindowTitle does not match '$ExpectedWindowTitle'."
Assert-ReportCondition ($report.nativeWindowObserved -eq $true) "nativeWindowObserved must be true."
Assert-ReportCondition ($report.processId -gt 0) "processId must be positive."
Assert-ReportCondition ($report.processExitCode -eq 0) "processExitCode must be 0."
Assert-ReportCondition ($report.userDataIsolated -eq $true) "userDataIsolated must be true."
Assert-ReportCondition ($report.userDataCleanupVerified -eq $true) "userDataCleanupVerified must be true."
Assert-ReportCondition (-not [string]::IsNullOrWhiteSpace([string]$report.userDataRoot)) "userDataRoot must be recorded."
Assert-ReportCondition (-not (Test-Path -LiteralPath ([string]$report.userDataRoot))) "the isolated userDataRoot still exists."
Assert-ReportCondition (-not [string]::IsNullOrWhiteSpace([string]$report.executablePath)) "executablePath must be recorded."
Assert-ReportCondition (Test-Path -LiteralPath ([string]$report.executablePath) -PathType Leaf) "the tested executable no longer exists."
Assert-ReportCondition ([System.IO.Path]::GetExtension([string]$report.executablePath) -eq ".exe") "the tested executable must be an .exe."

$startedAt = [DateTimeOffset]::Parse([string]$report.startedAtUtc)
$completedAt = [DateTimeOffset]::Parse([string]$report.completedAtUtc)
$wallDurationSeconds = ($completedAt - $startedAt).TotalSeconds
Assert-ReportCondition ($completedAt -ge $startedAt) "completedAtUtc precedes startedAtUtc."
Assert-ReportCondition ($wallDurationSeconds -ge $ExpectedDurationSeconds) "wall-clock duration was only $([Math]::Round($wallDurationSeconds, 2)) seconds."

$samples = @($report.samples)
$expectedSampleCount = [int][Math]::Ceiling($ExpectedDurationSeconds / [double]$report.sampleIntervalSeconds)
$minimumSampleCount = [Math]::Max(1, $expectedSampleCount - $AllowedSampleCountDrift)
$maximumSampleCount = $expectedSampleCount + $AllowedSampleCountDrift
Assert-ReportCondition ($samples.Count -ge $minimumSampleCount) "only $($samples.Count) samples were recorded; expected at least $minimumSampleCount."
Assert-ReportCondition ($samples.Count -le $maximumSampleCount) "$($samples.Count) samples were recorded; expected at most $maximumSampleCount."

$previousElapsedSeconds = -1
for ($index = 0; $index -lt $samples.Count; $index += 1) {
    $sample = $samples[$index]
    Assert-ReportCondition ($sample.elapsedSeconds -gt $previousElapsedSeconds) "sample $index does not have a strictly increasing elapsedSeconds value."
    Assert-ReportCondition ($sample.workingSetBytes -gt 0) "sample $index has an invalid workingSetBytes value."
    Assert-ReportCondition ($sample.privateMemoryBytes -gt 0) "sample $index has an invalid privateMemoryBytes value."
    Assert-ReportCondition ($sample.threadCount -gt 0) "sample $index has an invalid threadCount value."
    Assert-ReportCondition ($sample.windowTitle -eq $ExpectedWindowTitle) "sample $index has window title '$($sample.windowTitle)'."
    $previousElapsedSeconds = [int]$sample.elapsedSeconds
}

$minimumLastSampleElapsed = [Math]::Max(0, $ExpectedDurationSeconds - [int]$report.sampleIntervalSeconds - 1)
Assert-ReportCondition ($samples[-1].elapsedSeconds -ge $minimumLastSampleElapsed) "the last sample was captured too early at $($samples[-1].elapsedSeconds) seconds."

$workingSets = @($samples | ForEach-Object { [int64]$_.workingSetBytes })
$privateMemory = @($samples | ForEach-Object { [int64]$_.privateMemoryBytes })
$threads = @($samples | ForEach-Object { [int]$_.threadCount })
$summary = [ordered]@{
    status = "passed"
    reportPath = $resolvedReport
    requestedDurationSeconds = [int]$report.requestedDurationSeconds
    wallDurationSeconds = [Math]::Round($wallDurationSeconds, 2)
    sampleIntervalSeconds = [int]$report.sampleIntervalSeconds
    sampleCount = $samples.Count
    nativeWindowObserved = [bool]$report.nativeWindowObserved
    processExitCode = [int]$report.processExitCode
    userDataCleanupVerified = [bool]$report.userDataCleanupVerified
    firstWorkingSetBytes = $workingSets[0]
    lastWorkingSetBytes = $workingSets[-1]
    peakWorkingSetBytes = ($workingSets | Measure-Object -Maximum).Maximum
    firstPrivateMemoryBytes = $privateMemory[0]
    lastPrivateMemoryBytes = $privateMemory[-1]
    peakPrivateMemoryBytes = ($privateMemory | Measure-Object -Maximum).Maximum
    minimumThreadCount = ($threads | Measure-Object -Minimum).Minimum
    maximumThreadCount = ($threads | Measure-Object -Maximum).Maximum
}

$summary | ConvertTo-Json -Depth 3
