[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,

    [string]$InstallDirectory = (Join-Path ([System.IO.Path]::GetTempPath()) ("chatos-nsis-smoke-" + [guid]::NewGuid().ToString("N"))),

    [string]$ReportPath = (Join-Path ([System.IO.Path]::GetTempPath()) "chatos-windows-install-smoke.json"),

    [string]$ExpectedSidecarSha256,

    [ValidateRange(1, 20)]
    [int]$StartupRuns = 1,

    [ValidateRange(5, 120)]
    [int]$StartupTimeoutSeconds = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Assert-FileExists {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [Parameter(Mandatory = $true)]
        [string]$Description
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Description is missing: $Path"
    }
}

function Get-PortableRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$BasePath,
        [Parameter(Mandatory = $true)][string]$TargetPath
    )

    $getRelativePath = [System.IO.Path].GetMethod(
        "GetRelativePath",
        [System.Reflection.BindingFlags]"Public,Static",
        $null,
        [Type[]]@([string], [string]),
        $null
    )
    if ($null -ne $getRelativePath) {
        return $getRelativePath.Invoke($null, @($BasePath, $TargetPath)).Replace("\", "/")
    }

    $baseUri = [Uri]::new(((Resolve-Path -LiteralPath $BasePath).Path.TrimEnd("\") + "\"))
    $targetUri = [Uri]::new((Resolve-Path -LiteralPath $TargetPath).Path)
    return [Uri]::UnescapeDataString($baseUri.MakeRelativeUri($targetUri).ToString())
}

function Wait-FileRemoved {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path,

        [int]$TimeoutSeconds = 15
    )

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    while ((Test-Path -LiteralPath $Path) -and [DateTimeOffset]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }

    if (Test-Path -LiteralPath $Path) {
        throw "Uninstall left an executable behind: $Path"
    }
}

function Get-NearestRankPercentile {
    param(
        [Parameter(Mandatory = $true)][long[]]$Values,
        [Parameter(Mandatory = $true)][ValidateRange(0.01, 1.0)][double]$Percentile
    )

    if ($Values.Count -eq 0) {
        throw "Cannot calculate a percentile without values."
    }
    $sorted = @($Values | Sort-Object)
    $index = [int][Math]::Max(0, [Math]::Ceiling($Percentile * $sorted.Count) - 1)
    return $sorted[$index]
}

$resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
if (Test-Path -LiteralPath $InstallDirectory) {
    throw "Smoke install directory already exists; refusing to reuse it: $InstallDirectory"
}
$userDataRoot = "$InstallDirectory-user-data"
if (Test-Path -LiteralPath $userDataRoot) {
    throw "Smoke user-data directory already exists; refusing to reuse it: $userDataRoot"
}

$reportParent = Split-Path -Parent $ReportPath
if ([string]::IsNullOrWhiteSpace($reportParent) -eq $false) {
    New-Item -ItemType Directory -Path $reportParent -Force | Out-Null
}

$report = [ordered]@{
    schemaVersion = 2
    status = "running"
    startedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    completedAtUtc = $null
    installerName = [System.IO.Path]::GetFileName($resolvedInstaller)
    installerSha256 = (Get-FileHash -LiteralPath $resolvedInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
    applicationName = $null
    applicationVersion = $null
    sidecarSha256 = $null
    resources = @()
    installExitCode = $null
    installDurationMs = $null
    applicationExitCode = $null
    applicationRuns = @()
    startupProfile = "isolated-cold"
    startupMedianMs = $null
    startupP95Ms = $null
    userDataIsolated = $false
    userDataCleanupVerified = $false
    uninstallExitCode = $null
    uninstallDurationMs = $null
    error = $null
}

$applicationPath = $null
$sidecarPath = $null
$uninstallerPath = $null
$failure = $null

try {
    $installWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $install = Start-Process -FilePath $resolvedInstaller -ArgumentList @("/S", "/D=$InstallDirectory") -Wait -PassThru
    $installWatch.Stop()
    $report.installDurationMs = $installWatch.ElapsedMilliseconds
    $report.installExitCode = $install.ExitCode
    if ($install.ExitCode -ne 0) {
        throw "NSIS installer failed with exit code $($install.ExitCode)"
    }

    $sidecarPath = Join-Path $InstallDirectory "chatos_local_agent_host.exe"
    Assert-FileExists -Path $sidecarPath -Description "Installed Local Agent Host"

    $applications = @(Get-ChildItem -LiteralPath $InstallDirectory -File -Filter "*.exe" |
        Where-Object { $_.Name -ne "chatos_local_agent_host.exe" -and $_.Name -notmatch "^unins" })
    if ($applications.Count -ne 1) {
        throw "Expected exactly one installed ChatOS application executable, found $($applications.Count)"
    }
    $applicationPath = $applications[0].FullName

    $requiredResources = @(
        (Join-Path $InstallDirectory "agent-skill-examples.json"),
        (Join-Path $InstallDirectory "Pets\fengtuan\spritesheet.webp")
    )
    foreach ($resource in $requiredResources) {
        Assert-FileExists -Path $resource -Description "Installed application resource"
    }

    $sidecarHash = (Get-FileHash -LiteralPath $sidecarPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($ExpectedSidecarSha256) -eq $false -and
        $sidecarHash -ne $ExpectedSidecarSha256.ToLowerInvariant()) {
        throw "Installed Local Agent Host hash does not match the prepared sidecar"
    }

    $applicationVersion = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($applicationPath).ProductVersion
    $report.applicationName = [System.IO.Path]::GetFileName($applicationPath)
    $report.applicationVersion = $applicationVersion
    $report.sidecarSha256 = $sidecarHash
    $report.resources = @($requiredResources | ForEach-Object {
        Get-PortableRelativePath -BasePath $InstallDirectory -TargetPath $_
    })

    $previousEnvironment = @{}
    foreach ($name in @("APPDATA", "LOCALAPPDATA", "CHATOS_INSTALL_SMOKE_EXIT_AFTER_STARTUP")) {
        $previousEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
    }
    try {
        [Environment]::SetEnvironmentVariable("CHATOS_INSTALL_SMOKE_EXIT_AFTER_STARTUP", "1", "Process")
        for ($index = 1; $index -le $StartupRuns; $index++) {
            $runDataRoot = Join-Path $userDataRoot "run-$index"
            New-Item -ItemType Directory -Path $runDataRoot -Force | Out-Null
            [Environment]::SetEnvironmentVariable("APPDATA", $runDataRoot, "Process")
            [Environment]::SetEnvironmentVariable("LOCALAPPDATA", $runDataRoot, "Process")

            $startupWatch = [System.Diagnostics.Stopwatch]::StartNew()
            $launched = Start-Process -FilePath $applicationPath -PassThru
            if (-not $launched.WaitForExit($StartupTimeoutSeconds * 1000)) {
                Stop-Process -Id $launched.Id -Force -ErrorAction SilentlyContinue
                throw "Installed ChatOS application did not complete startup run $index within $StartupTimeoutSeconds seconds."
            }
            $startupWatch.Stop()
            $report.applicationExitCode = $launched.ExitCode
            $report.applicationRuns += [ordered]@{
                run = $index
                durationMs = $startupWatch.ElapsedMilliseconds
                exitCode = $launched.ExitCode
            }
            if ($launched.ExitCode -ne 0) {
                throw "Installed ChatOS application startup run $index exited with code $($launched.ExitCode)"
            }
        }
        $durations = [long[]]@($report.applicationRuns | ForEach-Object { $_.durationMs })
        $report.startupMedianMs = Get-NearestRankPercentile -Values $durations -Percentile 0.5
        $report.startupP95Ms = Get-NearestRankPercentile -Values $durations -Percentile 0.95
        $report.userDataIsolated = $true
    }
    finally {
        foreach ($name in $previousEnvironment.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previousEnvironment[$name], "Process")
        }
    }

    $uninstaller = @(Get-ChildItem -LiteralPath $InstallDirectory -File -Filter "unins*.exe")
    if ($uninstaller.Count -ne 1) {
        throw "Expected exactly one NSIS uninstaller, found $($uninstaller.Count)"
    }
    $uninstallerPath = $uninstaller[0].FullName
    $uninstallWatch = [System.Diagnostics.Stopwatch]::StartNew()
    $uninstall = Start-Process -FilePath $uninstallerPath -ArgumentList "/S" -Wait -PassThru
    $uninstallWatch.Stop()
    $report.uninstallDurationMs = $uninstallWatch.ElapsedMilliseconds
    $report.uninstallExitCode = $uninstall.ExitCode
    if ($uninstall.ExitCode -ne 0) {
        throw "NSIS uninstaller failed with exit code $($uninstall.ExitCode)"
    }

    Wait-FileRemoved -Path $applicationPath
    Wait-FileRemoved -Path $sidecarPath
    $report.status = "passed"
}
catch {
    $failure = $_
    $report.status = "failed"
    $report.error = $_.Exception.Message
}
finally {
    if ($null -eq $uninstallerPath -and (Test-Path -LiteralPath $InstallDirectory -PathType Container)) {
        $cleanupUninstallers = @(Get-ChildItem -LiteralPath $InstallDirectory -File -Filter "unins*.exe" -ErrorAction SilentlyContinue)
        if ($cleanupUninstallers.Count -eq 1) {
            $uninstallerPath = $cleanupUninstallers[0].FullName
        }
    }

    if ($report.status -ne "passed" -and $null -ne $uninstallerPath -and
        (Test-Path -LiteralPath $uninstallerPath -PathType Leaf)) {
        try {
            $cleanup = Start-Process -FilePath $uninstallerPath -ArgumentList "/S" -Wait -PassThru
            if ($null -eq $report.uninstallExitCode) {
                $report.uninstallExitCode = $cleanup.ExitCode
            }
        }
        catch {
            if ($null -eq $failure) {
                $failure = $_
                $report.status = "failed"
                $report.error = $_.Exception.Message
            }
        }
    }

    try {
        if (Test-Path -LiteralPath $userDataRoot -PathType Container) {
            [System.IO.Directory]::Delete($userDataRoot, $true)
        }
        $report.userDataCleanupVerified = -not (Test-Path -LiteralPath $userDataRoot)
        if (-not $report.userDataCleanupVerified) {
            throw "The isolated smoke user-data directory was not removed."
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
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ReportPath -Encoding utf8
    Write-Host "Windows install smoke report: $ReportPath"
}

if ($null -ne $failure) {
    throw $failure
}
