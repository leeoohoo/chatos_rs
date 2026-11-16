[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath,

    [string]$ReportPath = (Join-Path ([System.IO.Path]::GetTempPath()) "chatos-windows-long-stability.json"),

    [ValidateRange(30, 86400)]
    [int]$DurationSeconds = 28800,

    [ValidateRange(1, 300)]
    [int]$SampleIntervalSeconds = 30,

    [string]$ExpectedWindowTitle = "ChatOS"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Wait-NativeWindow {
    param(
        [Parameter(Mandatory = $true)][System.Diagnostics.Process]$Process,
        [Parameter(Mandatory = $true)][string]$ExpectedPath,
        [Parameter(Mandatory = $true)][string]$ExpectedTitle,
        [int]$TimeoutSeconds = 30
    )

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        if ($Process.HasExited) {
            return $false
        }
        try {
            $Process.Refresh()
            $pathMatches = [String]::Equals(
                $Process.MainModule.FileName,
                $ExpectedPath,
                [StringComparison]::OrdinalIgnoreCase
            )
            if ($pathMatches -and
                $Process.MainWindowHandle -ne [IntPtr]::Zero -and
                [String]::Equals($Process.MainWindowTitle, $ExpectedTitle, [StringComparison]::Ordinal)) {
                return $true
            }
        }
        catch {
            # The process can be between startup and WebView/window creation.
        }
        Start-Sleep -Milliseconds 250
    }
    return $false
}

function Get-DescendantProcessIds {
    param([Parameter(Mandatory = $true)][int]$RootProcessId)

    $all = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue)
    $pending = [System.Collections.Generic.Queue[int]]::new()
    $seen = [System.Collections.Generic.HashSet[int]]::new()
    $pending.Enqueue($RootProcessId)
    while ($pending.Count -gt 0) {
        $parent = $pending.Dequeue()
        foreach ($process in $all | Where-Object { $_.ParentProcessId -eq $parent }) {
            if ($seen.Add([int]$process.ProcessId)) {
                $pending.Enqueue([int]$process.ProcessId)
            }
        }
    }
    return @($seen)
}

function Stop-ProcessTree {
    param([Parameter(Mandatory = $true)][int]$RootProcessId)

    $descendants = @(Get-DescendantProcessIds -RootProcessId $RootProcessId)
    foreach ($processId in ($descendants | Sort-Object -Descending)) {
        Stop-Process -Id $processId -Force -ErrorAction SilentlyContinue
    }
    Stop-Process -Id $RootProcessId -Force -ErrorAction SilentlyContinue
}

function Assert-DirectoryIsNew {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (Test-Path -LiteralPath $Path) {
        throw "Refusing to reuse an existing stability directory: $Path"
    }
    New-Item -ItemType Directory -Path $Path -Force | Out-Null
}

$resolvedExecutable = (Resolve-Path -LiteralPath $ExecutablePath).Path
if (-not (Test-Path -LiteralPath $resolvedExecutable -PathType Leaf) -or
    [System.IO.Path]::GetExtension($resolvedExecutable) -ne ".exe") {
    throw "ExecutablePath must point to an existing .exe: $ExecutablePath"
}

$runRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("chatos-long-stability-" + [guid]::NewGuid().ToString("N"))
$userDataRoot = Join-Path $runRoot "user-data"
$reportParent = Split-Path -Parent $ReportPath
if (-not [string]::IsNullOrWhiteSpace($reportParent)) {
    New-Item -ItemType Directory -Path $reportParent -Force | Out-Null
}
Assert-DirectoryIsNew -Path $runRoot
Assert-DirectoryIsNew -Path $userDataRoot

$report = [ordered]@{
    schemaVersion = 1
    status = "running"
    startedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    completedAtUtc = $null
    executablePath = $resolvedExecutable
    expectedWindowTitle = $ExpectedWindowTitle
    requestedDurationSeconds = $DurationSeconds
    sampleIntervalSeconds = $SampleIntervalSeconds
    processId = $null
    nativeWindowObserved = $false
    samples = @()
    processExitCode = $null
    userDataRoot = $null
    userDataIsolated = $true
    userDataCleanupVerified = $false
    error = $null
}

$launched = $null
$failure = $null
$previousEnvironment = @{}
foreach ($name in @("APPDATA", "LOCALAPPDATA")) {
    $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
}

try {
    [Environment]::SetEnvironmentVariable("APPDATA", $userDataRoot, "Process")
    [Environment]::SetEnvironmentVariable("LOCALAPPDATA", $userDataRoot, "Process")
    $launched = Start-Process -FilePath $resolvedExecutable -PassThru
    $report.processId = $launched.Id
    $report.userDataRoot = $userDataRoot
    if (-not (Wait-NativeWindow -Process $launched -ExpectedPath $resolvedExecutable -ExpectedTitle $ExpectedWindowTitle)) {
        throw "Release executable did not expose a native '$ExpectedWindowTitle' window."
    }
    $report.nativeWindowObserved = $true

    $watch = [System.Diagnostics.Stopwatch]::StartNew()
    while ($watch.Elapsed.TotalSeconds -lt $DurationSeconds) {
        if ($launched.HasExited) {
            $report.processExitCode = $launched.ExitCode
            throw "Release executable exited during stability run with code $($launched.ExitCode)."
        }
        $launched.Refresh()
        $report.samples += [ordered]@{
            elapsedSeconds = [int][Math]::Floor($watch.Elapsed.TotalSeconds)
            workingSetBytes = $launched.WorkingSet64
            privateMemoryBytes = $launched.PrivateMemorySize64
            threadCount = $launched.Threads.Count
            windowTitle = $launched.MainWindowTitle
        }
        $remaining = $DurationSeconds - $watch.Elapsed.TotalSeconds
        if ($remaining -gt 0) {
            Start-Sleep -Seconds ([Math]::Max(1, [Math]::Min($SampleIntervalSeconds, [int][Math]::Ceiling($remaining))))
        }
    }
    $watch.Stop()
    $report.processExitCode = $null
    $report.status = "passed"
}
catch {
    $failure = $_
    $report.status = "failed"
    $report.error = $_.Exception.Message
}
finally {
    foreach ($name in $previousEnvironment.Keys) {
        [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], "Process")
    }
    if ($null -ne $launched) {
        try {
            if (-not $launched.HasExited) {
                $launched.CloseMainWindow() | Out-Null
                if (-not $launched.WaitForExit(10000)) {
                    Stop-ProcessTree -RootProcessId $launched.Id
                }
            }
            if ($launched.HasExited) {
                $report.processExitCode = $launched.ExitCode
            }
        }
        catch {
            if ($null -eq $failure) {
                $failure = $_
                $report.status = "failed"
                $report.error = $_.Exception.Message
            }
            Stop-ProcessTree -RootProcessId $launched.Id
        }
    }
    try {
        if (Test-Path -LiteralPath $runRoot) {
            [System.IO.Directory]::Delete($runRoot, $true)
        }
        $report.userDataCleanupVerified = -not (Test-Path -LiteralPath $runRoot)
        if (-not $report.userDataCleanupVerified) {
            throw "The isolated stability user-data directory was not removed."
        }
    }
    catch {
        if ($null -eq $failure) {
            $failure = $_
            $report.status = "failed"
            $report.error = $_.Exception.Message
        }
    }
    $report.completedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    $report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $ReportPath -Encoding utf8
    Write-Host "Windows long stability report: $ReportPath"
}

if ($null -ne $failure) {
    throw $failure
}
