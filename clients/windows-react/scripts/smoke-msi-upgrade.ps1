[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$LegacyInstallerPath,

    [Parameter(Mandatory = $true)]
    [string]$CurrentInstallerPath,

    [string]$LegacyVersion = "0.1.0",

    [string]$CurrentVersion = "3.0.9",

    [string]$InstallDirectory = (Join-Path ([System.IO.Path]::GetTempPath()) ("chatos-msi-upgrade-" + [guid]::NewGuid().ToString("N") + "\app")),

    [string]$ReportPath = (Join-Path ([System.IO.Path]::GetTempPath()) "chatos-windows-msi-upgrade-smoke.json"),

    [ValidateRange(5, 120)]
    [int]$StartupTimeoutSeconds = 20
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Get-MsiProperty {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Property
    )

    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.GetType().InvokeMember(
        "OpenDatabase", "InvokeMethod", $null, $installer, @($Path, 0)
    )
    $view = $database.GetType().InvokeMember(
        "OpenView", "InvokeMethod", $null, $database,
        @("SELECT ``Value`` FROM ``Property`` WHERE ``Property``='$Property'")
    )
    $view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null) | Out-Null
    $record = $view.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $view, $null)
    if ($null -eq $record) {
        throw "MSI property is missing: $Property"
    }
    return $record.GetType().InvokeMember("StringData", "GetProperty", $null, $record, 1)
}

function Get-MsiProductState {
    param([Parameter(Mandatory = $true)][string]$ProductCode)

    $installer = New-Object -ComObject WindowsInstaller.Installer
    return $installer.GetType().InvokeMember(
        "ProductState", "GetProperty", $null, $installer, $ProductCode
    )
}

function Get-MsiDowngradeMessage {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Version
    )

    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.GetType().InvokeMember(
        "OpenDatabase", "InvokeMethod", $null, $installer, @($Path, 0)
    )
    $upgradeView = $database.GetType().InvokeMember(
        "OpenView", "InvokeMethod", $null, $database,
        @("SELECT ``VersionMin``, ``VersionMax``, ``Attributes``, ``ActionProperty`` FROM ``Upgrade`` WHERE ``ActionProperty``='WIX_DOWNGRADE_DETECTED'")
    )
    $upgradeView.GetType().InvokeMember("Execute", "InvokeMethod", $null, $upgradeView, $null) | Out-Null
    $upgradeRecord = $upgradeView.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $upgradeView, $null)
    if ($null -eq $upgradeRecord) {
        throw "Current MSI does not declare a downgrade-detection rule."
    }
    $minimumVersion = $upgradeRecord.GetType().InvokeMember("StringData", "GetProperty", $null, $upgradeRecord, 1)
    $actionProperty = $upgradeRecord.GetType().InvokeMember("StringData", "GetProperty", $null, $upgradeRecord, 4)
    if ($minimumVersion -ne $Version -or $actionProperty -ne "WIX_DOWNGRADE_DETECTED") {
        throw "Current MSI downgrade-detection rule does not start at version $Version."
    }

    $conditionView = $database.GetType().InvokeMember(
        "OpenView", "InvokeMethod", $null, $database,
        @("SELECT ``Description`` FROM ``LaunchCondition`` WHERE ``Condition``='NOT WIX_DOWNGRADE_DETECTED'")
    )
    $conditionView.GetType().InvokeMember("Execute", "InvokeMethod", $null, $conditionView, $null) | Out-Null
    $conditionRecord = $conditionView.GetType().InvokeMember("Fetch", "InvokeMethod", $null, $conditionView, $null)
    if ($null -eq $conditionRecord) {
        throw "Current MSI does not fail closed when a newer version is detected."
    }
    return $conditionRecord.GetType().InvokeMember("StringData", "GetProperty", $null, $conditionRecord, 1)
}

function Invoke-MsiInstall {
    param(
        [Parameter(Mandatory = $true)][string]$InstallerPath,
        [Parameter(Mandatory = $true)][string]$Destination,
        [Parameter(Mandatory = $true)][string]$LogPath
    )

    $arguments = @(
        "/i", "`"$InstallerPath`"", "/qn", "/norestart",
        "ALLUSERS=2", "MSIINSTALLPERUSER=1", "INSTALLDIR=`"$Destination`"",
        "/L*v", "`"$LogPath`""
    ) -join " "
    return Start-Process -FilePath "msiexec.exe" -ArgumentList $arguments -Wait -PassThru
}

function Invoke-MsiUninstall {
    param([Parameter(Mandatory = $true)][string]$ProductCode)

    return Start-Process -FilePath "msiexec.exe" -ArgumentList "/x $ProductCode /qn /norestart" -Wait -PassThru
}

function Assert-SuccessCode {
    param(
        [Parameter(Mandatory = $true)][int]$ExitCode,
        [Parameter(Mandatory = $true)][string]$Operation
    )

    if ($ExitCode -notin @(0, 3010)) {
        throw "$Operation failed with Windows Installer exit code $ExitCode"
    }
}

function Find-Application {
    param([Parameter(Mandatory = $true)][string]$Directory)

    $applications = @(Get-ChildItem -LiteralPath $Directory -File -Filter "*.exe" |
        Where-Object { $_.Name -ne "chatos_local_agent_host.exe" })
    if ($applications.Count -ne 1) {
        throw "Expected exactly one ChatOS application executable, found $($applications.Count)"
    }
    return $applications[0].FullName
}

function Assert-ApplicationVersion {
    param(
        [Parameter(Mandatory = $true)][string]$ApplicationPath,
        [Parameter(Mandatory = $true)][string]$ExpectedVersion
    )

    $actual = [System.Diagnostics.FileVersionInfo]::GetVersionInfo($ApplicationPath).ProductVersion
    if ($actual -ne $ExpectedVersion) {
        throw "Expected installed application version $ExpectedVersion, received $actual"
    }
    return $actual
}

function Invoke-ApplicationSmoke {
    param(
        [Parameter(Mandatory = $true)][string]$ApplicationPath,
        [Parameter(Mandatory = $true)][string]$UserDataRoot,
        [Parameter(Mandatory = $true)][int]$TimeoutSeconds
    )

    $previous = @{}
    foreach ($name in @("APPDATA", "LOCALAPPDATA", "CHATOS_INSTALL_SMOKE_EXIT_AFTER_STARTUP")) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name, "Process")
    }
    [Environment]::SetEnvironmentVariable("APPDATA", $UserDataRoot, "Process")
    [Environment]::SetEnvironmentVariable("LOCALAPPDATA", $UserDataRoot, "Process")
    [Environment]::SetEnvironmentVariable("CHATOS_INSTALL_SMOKE_EXIT_AFTER_STARTUP", "1", "Process")
    try {
        $application = Start-Process -FilePath $ApplicationPath -PassThru
        if (-not $application.WaitForExit($TimeoutSeconds * 1000)) {
            Stop-Process -Id $application.Id -Force -ErrorAction SilentlyContinue
            throw "Installed ChatOS did not complete its graceful startup/exit smoke path."
        }
        if ($application.ExitCode -ne 0) {
            throw "Installed ChatOS exited with code $($application.ExitCode)"
        }
        return $application.ExitCode
    }
    finally {
        foreach ($name in $previous.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previous[$name], "Process")
        }
    }
}

function New-TruncatedInstaller {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    $inputStream = [System.IO.File]::OpenRead($Source)
    try {
        $outputStream = [System.IO.File]::Create($Destination)
        try {
            $buffer = [byte[]]::new(4096)
            $count = $inputStream.Read($buffer, 0, $buffer.Length)
            $outputStream.Write($buffer, 0, $count)
        }
        finally {
            $outputStream.Dispose()
        }
    }
    finally {
        $inputStream.Dispose()
    }
}

$resolvedLegacy = (Resolve-Path -LiteralPath $LegacyInstallerPath).Path
$resolvedCurrent = (Resolve-Path -LiteralPath $CurrentInstallerPath).Path
if ([string]::Equals($resolvedLegacy, $resolvedCurrent, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Legacy and current MSI paths must be different."
}
if (Test-Path -LiteralPath $InstallDirectory) {
    throw "Upgrade smoke install directory already exists; refusing to reuse it: $InstallDirectory"
}

$legacyProductCode = Get-MsiProperty -Path $resolvedLegacy -Property "ProductCode"
$currentProductCode = Get-MsiProperty -Path $resolvedCurrent -Property "ProductCode"
$legacyUpgradeCode = Get-MsiProperty -Path $resolvedLegacy -Property "UpgradeCode"
$currentUpgradeCode = Get-MsiProperty -Path $resolvedCurrent -Property "UpgradeCode"
$legacyMsiVersion = Get-MsiProperty -Path $resolvedLegacy -Property "ProductVersion"
$currentMsiVersion = Get-MsiProperty -Path $resolvedCurrent -Property "ProductVersion"
$downgradeMessage = Get-MsiDowngradeMessage -Path $resolvedCurrent -Version $CurrentVersion

if ($legacyMsiVersion -ne $LegacyVersion -or $currentMsiVersion -ne $CurrentVersion) {
    throw "MSI versions do not match the requested upgrade path."
}
if (-not [string]::Equals($legacyUpgradeCode, $currentUpgradeCode, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Legacy and current MSI packages do not share an UpgradeCode."
}
if ([version]$currentMsiVersion -le [version]$legacyMsiVersion) {
    throw "Current MSI version must be newer than the legacy MSI version."
}
foreach ($productCode in @($legacyProductCode, $currentProductCode)) {
    if ((Get-MsiProductState -ProductCode $productCode) -ne -1) {
        throw "Refusing to reuse an MSI ProductCode that is already installed: $productCode"
    }
}

$installRoot = Split-Path -Parent $InstallDirectory
$ownsInstallRoot = -not $PSBoundParameters.ContainsKey("InstallDirectory")
$reportParent = Split-Path -Parent $ReportPath
if (-not [string]::IsNullOrWhiteSpace($reportParent)) {
    New-Item -ItemType Directory -Path $reportParent -Force | Out-Null
}
New-Item -ItemType Directory -Path $installRoot -Force | Out-Null

$report = [ordered]@{
    schemaVersion = 1
    status = "running"
    startedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    completedAtUtc = $null
    legacyVersion = $legacyMsiVersion
    currentVersion = $currentMsiVersion
    upgradeCode = $currentUpgradeCode
    legacyInstallerSha256 = (Get-FileHash -LiteralPath $resolvedLegacy -Algorithm SHA256).Hash.ToLowerInvariant()
    currentInstallerSha256 = (Get-FileHash -LiteralPath $resolvedCurrent -Algorithm SHA256).Hash.ToLowerInvariant()
    legacyInstallExitCode = $null
    legacyApplicationExitCode = $null
    corruptedUpgradeExitCode = $null
    currentUpgradeExitCode = $null
    applicationExitCode = $null
    uninstallExitCode = $null
    sentinelPreservedAfterFailedUpgrade = $false
    sentinelPreservedAfterUpgrade = $false
    downgradeGuardVerified = $true
    downgradeMessage = $downgradeMessage
    error = $null
}

$sentinelContent = '{"owner":"upgrade-smoke","schemaVersion":1}'
$userDataRoot = Join-Path $installRoot "user-data"
$sentinelPath = Join-Path $userDataRoot "com.chatos.windows\upgrade-smoke-user-data.json"
$applicationPath = $null
$failure = $null

try {
    $legacyLog = Join-Path $installRoot "legacy-install.log"
    $legacyInstall = Invoke-MsiInstall -InstallerPath $resolvedLegacy -Destination $InstallDirectory -LogPath $legacyLog
    $report.legacyInstallExitCode = $legacyInstall.ExitCode
    Assert-SuccessCode -ExitCode $legacyInstall.ExitCode -Operation "Legacy MSI installation"

    $applicationPath = Find-Application -Directory $InstallDirectory
    $null = Assert-ApplicationVersion -ApplicationPath $applicationPath -ExpectedVersion $LegacyVersion
    $legacyApplicationHash = (Get-FileHash -LiteralPath $applicationPath -Algorithm SHA256).Hash
    $report.legacyApplicationExitCode = Invoke-ApplicationSmoke -ApplicationPath $applicationPath -UserDataRoot $userDataRoot -TimeoutSeconds $StartupTimeoutSeconds
    New-Item -ItemType Directory -Path (Split-Path -Parent $sentinelPath) -Force | Out-Null
    Set-Content -LiteralPath $sentinelPath -Value $sentinelContent -Encoding utf8NoBOM

    $corruptedInstaller = Join-Path $installRoot "truncated-current.msi"
    New-TruncatedInstaller -Source $resolvedCurrent -Destination $corruptedInstaller
    $corruptLog = Join-Path $installRoot "corrupted-upgrade.log"
    $corruptUpgrade = Invoke-MsiInstall -InstallerPath $corruptedInstaller -Destination $InstallDirectory -LogPath $corruptLog
    $report.corruptedUpgradeExitCode = $corruptUpgrade.ExitCode
    if ($corruptUpgrade.ExitCode -in @(0, 3010)) {
        throw "The intentionally truncated upgrade MSI unexpectedly succeeded."
    }
    if ((Get-FileHash -LiteralPath $applicationPath -Algorithm SHA256).Hash -ne $legacyApplicationHash) {
        throw "A rejected upgrade changed the installed legacy application."
    }
    $null = Assert-ApplicationVersion -ApplicationPath $applicationPath -ExpectedVersion $LegacyVersion
    $report.sentinelPreservedAfterFailedUpgrade = (Get-Content -LiteralPath $sentinelPath -Raw).Trim() -eq $sentinelContent
    if (-not $report.sentinelPreservedAfterFailedUpgrade) {
        throw "A rejected upgrade changed the user-data sentinel."
    }

    $upgradeLog = Join-Path $installRoot "current-upgrade.log"
    $currentUpgrade = Invoke-MsiInstall -InstallerPath $resolvedCurrent -Destination $InstallDirectory -LogPath $upgradeLog
    $report.currentUpgradeExitCode = $currentUpgrade.ExitCode
    Assert-SuccessCode -ExitCode $currentUpgrade.ExitCode -Operation "Current MSI upgrade"

    $applicationPath = Find-Application -Directory $InstallDirectory
    $null = Assert-ApplicationVersion -ApplicationPath $applicationPath -ExpectedVersion $CurrentVersion
    $report.sentinelPreservedAfterUpgrade = (Get-Content -LiteralPath $sentinelPath -Raw).Trim() -eq $sentinelContent
    if (-not $report.sentinelPreservedAfterUpgrade) {
        throw "The successful upgrade changed the user-data sentinel."
    }

    $report.applicationExitCode = Invoke-ApplicationSmoke -ApplicationPath $applicationPath -UserDataRoot $userDataRoot -TimeoutSeconds $StartupTimeoutSeconds

    $uninstall = Invoke-MsiUninstall -ProductCode $currentProductCode
    $report.uninstallExitCode = $uninstall.ExitCode
    Assert-SuccessCode -ExitCode $uninstall.ExitCode -Operation "Current MSI uninstall"
    if (Test-Path -LiteralPath $applicationPath -PathType Leaf) {
        throw "MSI uninstall left the application executable behind."
    }
    $report.status = "passed"
}
catch {
    $failure = $_
    $report.status = "failed"
    $report.error = $_.Exception.Message
}
finally {
    foreach ($productCode in @($currentProductCode, $legacyProductCode)) {
        if ((Get-MsiProductState -ProductCode $productCode) -ne -1) {
            $cleanup = Invoke-MsiUninstall -ProductCode $productCode
            if ($cleanup.ExitCode -notin @(0, 1605, 3010) -and $null -eq $failure) {
                $failure = [System.InvalidOperationException]::new("MSI cleanup failed with exit code $($cleanup.ExitCode)")
                $report.status = "failed"
                $report.error = $failure.Message
            }
        }
    }

    $report.completedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    $report | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ReportPath -Encoding utf8
    Write-Host "Windows MSI upgrade smoke report: $ReportPath"

    if ($ownsInstallRoot -and (Test-Path -LiteralPath $installRoot -PathType Container)) {
        $tempRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd("\") + "\"
        $resolvedRoot = [System.IO.Path]::GetFullPath($installRoot)
        $rootName = [System.IO.Path]::GetFileName($resolvedRoot)
        if (-not $resolvedRoot.StartsWith($tempRoot, [System.StringComparison]::OrdinalIgnoreCase) -or
            -not $rootName.StartsWith("chatos-msi-upgrade-", [System.StringComparison]::Ordinal)) {
            throw "Refusing to clean an unexpected MSI smoke directory: $resolvedRoot"
        }
        [System.IO.Directory]::Delete($resolvedRoot, $true)
    }
}

if ($null -ne $failure) {
    throw $failure
}
