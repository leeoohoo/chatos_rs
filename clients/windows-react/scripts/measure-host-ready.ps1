[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath,

    [string]$ReportPath = (Join-Path ([System.IO.Path]::GetTempPath()) "chatos-windows-host-ready.json"),

    [ValidateRange(3, 100)]
    [int]$Iterations = 10,

    [ValidateRange(500, 30000)]
    [int]$ReadyTimeoutMilliseconds = 5000
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Quote-ProcessArgument {
    param([Parameter(Mandatory = $true)][string]$Value)
    return '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

function Read-ExactBeforeDeadline {
    param(
        [Parameter(Mandatory = $true)]$Stream,
        [Parameter(Mandatory = $true)][int]$Count,
        [Parameter(Mandatory = $true)][DateTimeOffset]$Deadline
    )

    $buffer = New-Object byte[] $Count
    $offset = 0
    while ($offset -lt $Count) {
        $remaining = [int][Math]::Floor(($Deadline - [DateTimeOffset]::UtcNow).TotalMilliseconds)
        if ($remaining -le 0) {
            throw "Local Agent Host did not become ready before the timeout."
        }
        $readTask = $Stream.ReadAsync($buffer, $offset, $Count - $offset)
        if (-not $readTask.Wait($remaining)) {
            throw "Local Agent Host did not become ready before the timeout."
        }
        $read = $readTask.Result
        if ($read -le 0) {
            throw "Local Agent Host closed stdout before the health response completed."
        }
        $offset += $read
    }
    return $buffer
}

function Invoke-HealthHandshake {
    param(
        [Parameter(Mandatory = $true)]$Process,
        [Parameter(Mandatory = $true)][int]$TimeoutMilliseconds,
        [Parameter(Mandatory = $true)][int]$Sequence
    )

    $deadline = [DateTimeOffset]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
    $request = [ordered]@{
        protocol_version = 53
        command_id = "host-ready-$Sequence"
        command = @{ type = "health" }
    }
    $body = [Text.Encoding]::UTF8.GetBytes(($request | ConvertTo-Json -Depth 6 -Compress))
    $prefix = [BitConverter]::GetBytes([uint32]$body.Length)
    [Array]::Reverse($prefix)
    $input = $Process.StandardInput.BaseStream
    $input.Write($prefix, 0, $prefix.Length)
    $input.Write($body, 0, $body.Length)
    $input.Flush()

    $output = $Process.StandardOutput.BaseStream
    $responsePrefix = Read-ExactBeforeDeadline -Stream $output -Count 4 -Deadline $deadline
    [Array]::Reverse($responsePrefix)
    $responseLength = [BitConverter]::ToUInt32($responsePrefix, 0)
    if ($responseLength -eq 0 -or $responseLength -gt (64 * 1024 * 1024)) {
        throw "Local Agent Host returned an invalid health response length: $responseLength"
    }
    $responseBytes = Read-ExactBeforeDeadline -Stream $output -Count ([int]$responseLength) -Deadline $deadline
    return [Text.Encoding]::UTF8.GetString($responseBytes) | ConvertFrom-Json
}

function Get-NearestRankPercentile {
    param(
        [Parameter(Mandatory = $true)][long[]]$Values,
        [Parameter(Mandatory = $true)][ValidateRange(0.01, 1.0)][double]$Percentile
    )

    $sorted = @($Values | Sort-Object)
    $rank = [Math]::Max(1, [Math]::Ceiling($Percentile * $sorted.Count))
    return [long]$sorted[$rank - 1]
}

function Write-Report {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][object]$Value
    )

    $parent = Split-Path -Parent $Path
    if (-not [string]::IsNullOrWhiteSpace($parent)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $temporary = "$Path.$([guid]::NewGuid().ToString('N')).tmp"
    try {
        $Value | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $temporary -Encoding utf8
        Move-Item -LiteralPath $temporary -Destination $Path -Force
    }
    finally {
        if (Test-Path -LiteralPath $temporary) {
            Remove-Item -LiteralPath $temporary -Force -ErrorAction SilentlyContinue
        }
    }
}

$resolvedExecutable = (Resolve-Path -LiteralPath $ExecutablePath -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $resolvedExecutable -PathType Leaf) -or
    [System.IO.Path]::GetExtension($resolvedExecutable) -ne ".exe") {
    throw "ExecutablePath must point to an existing .exe: $ExecutablePath"
}

$report = [ordered]@{
    schemaVersion = 1
    status = "running"
    startedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    completedAtUtc = $null
    executablePath = $resolvedExecutable
    executableSha256 = (Get-FileHash -LiteralPath $resolvedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    protocolVersion = 53
    iterations = $Iterations
    readyTimeoutMilliseconds = $ReadyTimeoutMilliseconds
    samples = @()
    p50Milliseconds = $null
    p95Milliseconds = $null
    maximumMilliseconds = $null
    cleanupVerified = $false
    error = $null
}

$runRoots = [System.Collections.Generic.List[string]]::new()
$failure = $null
try {
    for ($index = 1; $index -le $Iterations; $index += 1) {
        $runRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("chatos-host-ready-" + [guid]::NewGuid().ToString("N"))
        if (Test-Path -LiteralPath $runRoot) {
            throw "Refusing to reuse a Host ready directory: $runRoot"
        }
        New-Item -ItemType Directory -Path $runRoot | Out-Null
        $runRoots.Add($runRoot)
        $databasePath = Join-Path $runRoot "host.sqlite"

        $startInfo = New-Object System.Diagnostics.ProcessStartInfo
        $startInfo.FileName = $resolvedExecutable
        $startInfo.Arguments = @(
            "--database", (Quote-ProcessArgument $databasePath),
            "--owner-user-id", "host-ready-baseline",
            "--disable-workers",
            "--stdio"
        ) -join " "
        $startInfo.UseShellExecute = $false
        $startInfo.CreateNoWindow = $true
        $startInfo.RedirectStandardInput = $true
        $startInfo.RedirectStandardOutput = $true
        $startInfo.RedirectStandardError = $true

        $process = New-Object System.Diagnostics.Process
        $process.StartInfo = $startInfo
        $watch = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            if (-not $process.Start()) {
                throw "Local Agent Host failed to start on iteration $index."
            }
            $response = Invoke-HealthHandshake -Process $process -TimeoutMilliseconds $ReadyTimeoutMilliseconds -Sequence $index
            $watch.Stop()
            if (-not $response.ok -or $response.result.type -ne "health") {
                $responseText = $response | ConvertTo-Json -Depth 8 -Compress
                throw "Local Agent Host returned an invalid health response on iteration ${index}: $responseText"
            }
            $process.StandardInput.Close()
            if (-not $process.WaitForExit(10000)) {
                $process.Kill()
                $process.WaitForExit()
                throw "Local Agent Host did not exit after stdio closed on iteration $index."
            }
            if ($process.ExitCode -ne 0) {
                throw "Local Agent Host exited with code $($process.ExitCode) on iteration $index."
            }
            $report.samples += [ordered]@{
                iteration = $index
                readyMilliseconds = $watch.ElapsedMilliseconds
                processExitCode = $process.ExitCode
                databaseCreated = Test-Path -LiteralPath $databasePath -PathType Leaf
            }
        }
        finally {
            $watch.Stop()
            if ($null -ne $process) {
                try {
                    if (-not $process.HasExited) {
                        $process.Kill()
                        $process.WaitForExit()
                    }
                }
                catch { }
                $process.Dispose()
            }
            if (Test-Path -LiteralPath $runRoot) {
                [System.IO.Directory]::Delete($runRoot, $true)
            }
        }
    }

    $durations = [long[]]@($report.samples | ForEach-Object { $_.readyMilliseconds })
    $report.p50Milliseconds = Get-NearestRankPercentile -Values $durations -Percentile 0.5
    $report.p95Milliseconds = Get-NearestRankPercentile -Values $durations -Percentile 0.95
    $report.maximumMilliseconds = ($durations | Measure-Object -Maximum).Maximum
    $report.cleanupVerified = -not ($runRoots | Where-Object { Test-Path -LiteralPath $_ })
    if (-not $report.cleanupVerified) {
        throw "One or more Host ready temporary directories remain."
    }
    $report.status = "passed"
}
catch {
    $failure = $_
    $report.status = "failed"
    $report.error = $_.Exception.Message
}
finally {
    foreach ($runRoot in $runRoots) {
        if (Test-Path -LiteralPath $runRoot) {
            try { [System.IO.Directory]::Delete($runRoot, $true) } catch { }
        }
    }
    $report.cleanupVerified = -not ($runRoots | Where-Object { Test-Path -LiteralPath $_ })
    $report.completedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    Write-Report -Path $ReportPath -Value $report
    Write-Host "Windows Host ready report: $ReportPath"
}

if ($null -ne $failure) {
    throw $failure
}
