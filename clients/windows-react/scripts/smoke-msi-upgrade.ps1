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

function Get-RememberedInstallDirectoryState {
    $registryPath = "Software\chatos\ChatOS"
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey($registryPath, $false)
    if ($null -eq $key) {
        return [pscustomobject]@{
            KeyExisted = $false
            HasDefault = $false
            DefaultValue = $null
            DefaultKind = $null
            HasNamed = $false
            NamedValue = $null
            NamedKind = $null
        }
    }
    try {
        $names = @($key.GetValueNames())
        $hasDefault = $names -contains ""
        $hasNamed = $names -contains "InstallDir"
        return [pscustomobject]@{
            KeyExisted = $true
            HasDefault = $hasDefault
            DefaultValue = if ($hasDefault) { $key.GetValue("") } else { $null }
            DefaultKind = if ($hasDefault) { $key.GetValueKind("") } else { $null }
            HasNamed = $hasNamed
            NamedValue = if ($hasNamed) { $key.GetValue("InstallDir") } else { $null }
            NamedKind = if ($hasNamed) { $key.GetValueKind("InstallDir") } else { $null }
        }
    }
    finally {
        $key.Dispose()
    }
}

function Clear-RememberedInstallDirectory {
    param([Parameter(Mandatory = $true)]$State)

    foreach ($value in @($State.DefaultValue, $State.NamedValue)) {
        if ($value -is [string] -and -not [string]::IsNullOrWhiteSpace($value)) {
            $candidate = [System.IO.Path]::GetFullPath($value.Trim())
            if (Test-Path -LiteralPath $candidate) {
                throw "Refusing to replace an active remembered ChatOS install directory: $candidate"
            }
        }
    }
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Software\chatos\ChatOS", $true)
    if ($null -eq $key) {
        return
    }
    try {
        $key.DeleteValue("", $false)
        $key.DeleteValue("InstallDir", $false)
    }
    finally {
        $key.Dispose()
    }
}

function Wait-RememberedInstallDirectoryCleared {
    param([Parameter(Mandatory = $true)]$OriginalState)

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds(20)
    $stableObservations = 0
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        Clear-RememberedInstallDirectory -State $OriginalState
        Start-Sleep -Milliseconds 250
        $current = Get-RememberedInstallDirectoryState
        if (-not $current.HasDefault -and -not $current.HasNamed) {
            $stableObservations++
            if ($stableObservations -ge 20) {
                return
            }
        }
        else {
            $stableObservations = 0
        }
    }
    throw "The remembered ChatOS install directory did not remain cleared."
}

function Restore-RememberedInstallDirectory {
    param([Parameter(Mandatory = $true)]$State)

    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey("Software\chatos\ChatOS", $true)
    try {
        if ($State.HasDefault) {
            $key.SetValue("", $State.DefaultValue, $State.DefaultKind)
        }
        else {
            $key.DeleteValue("", $false)
        }
        if ($State.HasNamed) {
            $key.SetValue("InstallDir", $State.NamedValue, $State.NamedKind)
        }
        else {
            $key.DeleteValue("InstallDir", $false)
        }
    }
    finally {
        $key.Dispose()
    }
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
    param(
        [Parameter(Mandatory = $true)][string]$ProductCode,
        [string]$LogPath
    )

    $arguments = "/x $ProductCode /qn /norestart"
    if (-not [string]::IsNullOrWhiteSpace($LogPath)) {
        $arguments += " /L*v `"$LogPath`""
    }
    return Start-Process -FilePath "msiexec.exe" -ArgumentList $arguments -Wait -PassThru
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

function Wait-FileRemoved {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [ValidateRange(1, 60)][int]$TimeoutSeconds = 15
    )

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    while ((Test-Path -LiteralPath $Path -PathType Leaf) -and [DateTimeOffset]::UtcNow -lt $deadline) {
        Start-Sleep -Milliseconds 250
    }
    if (Test-Path -LiteralPath $Path -PathType Leaf) {
        throw "MSI uninstall left the application executable behind."
    }
}

function Find-Application {
    param([Parameter(Mandatory = $true)][string]$Directory)

    if (-not (Test-Path -LiteralPath $Directory -PathType Container)) {
        throw "Installed application directory is missing: $Directory"
    }
    $applications = @(Get-ChildItem -LiteralPath $Directory -File -Filter "*.exe" |
        Where-Object { $_.Name -ne "chatos_local_agent_host.exe" })
    if ($applications.Count -ne 1) {
        throw "Expected exactly one ChatOS application executable, found $($applications.Count)"
    }
    return $applications[0].FullName
}

function Wait-ApplicationInstalled {
    param(
        [Parameter(Mandatory = $true)][string]$Directory,
        [ValidateRange(1, 60)][int]$TimeoutSeconds = 15
    )

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        if (Test-Path -LiteralPath $Directory -PathType Container) {
            $applications = @(Get-ChildItem -LiteralPath $Directory -File -Filter "*.exe" -ErrorAction SilentlyContinue |
                Where-Object { $_.Name -ne "chatos_local_agent_host.exe" })
            if ($applications.Count -gt 0) {
                return Find-Application -Directory $Directory
            }
        }
        Start-Sleep -Milliseconds 250
    }
    throw "Installed application did not appear before the timeout."
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

function Start-ApplicationForRunningUpgrade {
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
    [Environment]::SetEnvironmentVariable("CHATOS_INSTALL_SMOKE_EXIT_AFTER_STARTUP", $null, "Process")
    try {
        $application = Start-Process -FilePath $ApplicationPath -PassThru
    }
    finally {
        foreach ($name in $previous.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previous[$name], "Process")
        }
    }

    $deadline = [DateTimeOffset]::UtcNow.AddSeconds($TimeoutSeconds)
    while ([DateTimeOffset]::UtcNow -lt $deadline) {
        $application.Refresh()
        if ($application.HasExited) {
            throw "Installed ChatOS exited before the running-application upgrade began."
        }
        if ($application.MainWindowHandle -ne [IntPtr]::Zero) {
            return $application
        }
        Start-Sleep -Milliseconds 250
    }
    Stop-Process -Id $application.Id -Force -ErrorAction SilentlyContinue
    throw "Installed ChatOS did not expose a native window before the running-application upgrade."
}

function Get-PendingFileRenameState {
    $key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey(
        "SYSTEM\CurrentControlSet\Control\Session Manager",
        $false
    )
    try {
        $names = @($key.GetValueNames())
        if ($names -notcontains "PendingFileRenameOperations") {
            return '{"present":false}'
        }
        $operations = @($key.GetValue(
            "PendingFileRenameOperations",
            $null,
            [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames
        ))
        return ([ordered]@{
            present = $true
            operations = $operations
        } | ConvertTo-Json -Compress -Depth 3)
    }
    finally {
        $key.Dispose()
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

function New-IsolatedLegacyInstaller {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Copy-Item -LiteralPath $Source -Destination $Destination
    $installer = $null
    $database = $null
    $view = $null
    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.GetType().InvokeMember(
            "OpenDatabase", "InvokeMethod", $null, $installer, @($Destination, 1)
        )
        $view = $database.GetType().InvokeMember(
            "OpenView", "InvokeMethod", $null, $database,
            @("DELETE FROM ``AppSearch`` WHERE ``Property``='INSTALLDIR'")
        )
        $view.GetType().InvokeMember("Execute", "InvokeMethod", $null, $view, $null) | Out-Null
        $database.GetType().InvokeMember("Commit", "InvokeMethod", $null, $database, $null) | Out-Null
    }
    finally {
        foreach ($comObject in @($view, $database, $installer)) {
            if ($null -ne $comObject -and [System.Runtime.InteropServices.Marshal]::IsComObject($comObject)) {
                [System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($comObject) | Out-Null
            }
        }
    }
}

function New-RollbackFaultInstaller {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    Copy-Item -LiteralPath $Source -Destination $Destination
    $faultProductCode = "{" + [guid]::NewGuid().ToString().ToUpperInvariant() + "}"
    $installer = $null
    $database = $null
    $productView = $null
    $customActionView = $null
    $sequenceView = $null
    try {
        $installer = New-Object -ComObject WindowsInstaller.Installer
        $database = $installer.GetType().InvokeMember(
            "OpenDatabase", "InvokeMethod", $null, $installer, @($Destination, 1)
        )
        $productView = $database.GetType().InvokeMember(
            "OpenView", "InvokeMethod", $null, $database,
            @("UPDATE ``Property`` SET ``Value``='$faultProductCode' WHERE ``Property``='ProductCode'")
        )
        $productView.GetType().InvokeMember("Execute", "InvokeMethod", $null, $productView, $null) | Out-Null
        $customActionView = $database.GetType().InvokeMember(
            "OpenView", "InvokeMethod", $null, $database,
            @("INSERT INTO ``CustomAction`` (``Action``, ``Type``, ``Source``, ``Target``, ``ExtendedType``) VALUES ('ChatOSRollbackFault', 19, '', 'ChatOS rollback fault injection.', 0)")
        )
        $customActionView.GetType().InvokeMember("Execute", "InvokeMethod", $null, $customActionView, $null) | Out-Null
        $sequenceView = $database.GetType().InvokeMember(
            "OpenView", "InvokeMethod", $null, $database,
            @("INSERT INTO ``InstallExecuteSequence`` (``Action``, ``Condition``, ``Sequence``) VALUES ('ChatOSRollbackFault', 'NOT Installed', 4550)")
        )
        $sequenceView.GetType().InvokeMember("Execute", "InvokeMethod", $null, $sequenceView, $null) | Out-Null
        $database.GetType().InvokeMember("Commit", "InvokeMethod", $null, $database, $null) | Out-Null
    }
    finally {
        foreach ($comObject in @($sequenceView, $customActionView, $productView, $database, $installer)) {
            if ($null -ne $comObject -and [System.Runtime.InteropServices.Marshal]::IsComObject($comObject)) {
                [System.Runtime.InteropServices.Marshal]::FinalReleaseComObject($comObject) | Out-Null
            }
        }
    }
    return $faultProductCode
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
$rememberedInstallDirectoryState = Get-RememberedInstallDirectoryState
Wait-RememberedInstallDirectoryCleared -OriginalState $rememberedInstallDirectoryState

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
    transactionalRollbackExitCode = $null
    transactionalRollbackProductCode = $null
    currentUpgradeExitCode = $null
    applicationExitCode = $null
    uninstallExitCode = $null
    runningUpgradeLegacyInstallExitCode = $null
    runningUpgradeLegacyProcessId = $null
    runningUpgradeExitCode = $null
    runningUpgradeApplicationExitCode = $null
    runningUpgradeUninstallExitCode = $null
    runningUpgradeLegacyProcessExited = $false
    runningUpgradePendingRenameUnchanged = $false
    runningUpgradeVerified = $false
    sentinelPreservedAfterFailedUpgrade = $false
    transactionalRollbackVerified = $false
    sentinelPreservedAfterUpgrade = $false
    downgradeGuardVerified = $true
    downgradeMessage = $downgradeMessage
    error = $null
}

$sentinelContent = '{"owner":"upgrade-smoke","schemaVersion":1}'
$userDataRoot = Join-Path $installRoot "user-data"
$sentinelPath = Join-Path $userDataRoot "com.chatos.windows\upgrade-smoke-user-data.json"
$applicationPath = $null
$rollbackProductCode = $null
$runningLegacyProcess = $null
$failure = $null

try {
    $legacyLog = Join-Path $installRoot "legacy-install.log"
    $isolatedLegacyInstaller = Join-Path $installRoot "isolated-legacy.msi"
    New-IsolatedLegacyInstaller -Source $resolvedLegacy -Destination $isolatedLegacyInstaller
    $legacyInstall = Invoke-MsiInstall -InstallerPath $isolatedLegacyInstaller -Destination $InstallDirectory -LogPath $legacyLog
    $report.legacyInstallExitCode = $legacyInstall.ExitCode
    Assert-SuccessCode -ExitCode $legacyInstall.ExitCode -Operation "Legacy MSI installation"

    $applicationPath = Wait-ApplicationInstalled -Directory $InstallDirectory
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

    $rollbackInstaller = Join-Path $installRoot "rollback-fault-current.msi"
    $rollbackProductCode = New-RollbackFaultInstaller -Source $resolvedCurrent -Destination $rollbackInstaller
    $report.transactionalRollbackProductCode = $rollbackProductCode
    $rollbackLog = Join-Path $installRoot "transactional-rollback.log"
    $rollbackUpgrade = Invoke-MsiInstall -InstallerPath $rollbackInstaller -Destination $InstallDirectory -LogPath $rollbackLog
    $report.transactionalRollbackExitCode = $rollbackUpgrade.ExitCode
    if ($rollbackUpgrade.ExitCode -in @(0, 3010)) {
        throw "The transaction fault-injection upgrade unexpectedly succeeded."
    }
    $rollbackLogContent = Get-Content -LiteralPath $rollbackLog -Raw
    if (-not $rollbackLogContent.Contains("ChatOSRollbackFault")) {
        throw "The MSI transaction did not reach the injected post-copy failure action."
    }
    if ((Get-MsiProductState -ProductCode $legacyProductCode) -eq -1 -or
        (Get-MsiProductState -ProductCode $rollbackProductCode) -ne -1 -or
        (Get-MsiProductState -ProductCode $currentProductCode) -ne -1) {
        throw "Windows Installer did not restore the legacy product registration after rollback."
    }
    $applicationPath = Wait-ApplicationInstalled -Directory $InstallDirectory
    $null = Assert-ApplicationVersion -ApplicationPath $applicationPath -ExpectedVersion $LegacyVersion
    if ((Get-FileHash -LiteralPath $applicationPath -Algorithm SHA256).Hash -ne $legacyApplicationHash) {
        throw "Windows Installer did not restore the legacy application bytes after rollback."
    }
    if ((Get-Content -LiteralPath $sentinelPath -Raw).Trim() -ne $sentinelContent) {
        throw "The failed MSI transaction changed the AppData sentinel."
    }
    $report.transactionalRollbackVerified = $true

    $upgradeLog = Join-Path $installRoot "current-upgrade.log"
    $currentUpgrade = Invoke-MsiInstall -InstallerPath $resolvedCurrent -Destination $InstallDirectory -LogPath $upgradeLog
    $report.currentUpgradeExitCode = $currentUpgrade.ExitCode
    Assert-SuccessCode -ExitCode $currentUpgrade.ExitCode -Operation "Current MSI upgrade"

    $applicationPath = Wait-ApplicationInstalled -Directory $InstallDirectory
    $null = Assert-ApplicationVersion -ApplicationPath $applicationPath -ExpectedVersion $CurrentVersion
    $report.sentinelPreservedAfterUpgrade = (Get-Content -LiteralPath $sentinelPath -Raw).Trim() -eq $sentinelContent
    if (-not $report.sentinelPreservedAfterUpgrade) {
        throw "The successful upgrade changed the user-data sentinel."
    }

    $report.applicationExitCode = Invoke-ApplicationSmoke -ApplicationPath $applicationPath -UserDataRoot $userDataRoot -TimeoutSeconds $StartupTimeoutSeconds

    $uninstallLog = Join-Path $installRoot "current-uninstall.log"
    $uninstall = Invoke-MsiUninstall -ProductCode $currentProductCode -LogPath $uninstallLog
    $report.uninstallExitCode = $uninstall.ExitCode
    Assert-SuccessCode -ExitCode $uninstall.ExitCode -Operation "Current MSI uninstall"
    Wait-FileRemoved -Path $applicationPath

    $runningLegacyLog = Join-Path $installRoot "running-legacy-install.log"
    $runningLegacyInstall = Invoke-MsiInstall -InstallerPath $isolatedLegacyInstaller -Destination $InstallDirectory -LogPath $runningLegacyLog
    $report.runningUpgradeLegacyInstallExitCode = $runningLegacyInstall.ExitCode
    Assert-SuccessCode -ExitCode $runningLegacyInstall.ExitCode -Operation "Running-upgrade legacy MSI installation"

    $applicationPath = Wait-ApplicationInstalled -Directory $InstallDirectory
    $null = Assert-ApplicationVersion -ApplicationPath $applicationPath -ExpectedVersion $LegacyVersion
    $runningLegacyProcess = Start-ApplicationForRunningUpgrade -ApplicationPath $applicationPath -UserDataRoot $userDataRoot -TimeoutSeconds $StartupTimeoutSeconds
    $report.runningUpgradeLegacyProcessId = $runningLegacyProcess.Id
    $pendingRenameBefore = Get-PendingFileRenameState

    $runningUpgradeLog = Join-Path $installRoot "running-application-upgrade.log"
    $runningUpgrade = Invoke-MsiInstall -InstallerPath $resolvedCurrent -Destination $InstallDirectory -LogPath $runningUpgradeLog
    $report.runningUpgradeExitCode = $runningUpgrade.ExitCode
    if ($runningUpgrade.ExitCode -eq 3010) {
        throw "Running-application MSI upgrade requested a reboot."
    }
    Assert-SuccessCode -ExitCode $runningUpgrade.ExitCode -Operation "Running-application MSI upgrade"
    $pendingRenameAfter = Get-PendingFileRenameState
    $report.runningUpgradePendingRenameUnchanged = $pendingRenameBefore -ceq $pendingRenameAfter
    if (-not $report.runningUpgradePendingRenameUnchanged) {
        throw "Running-application MSI upgrade changed PendingFileRenameOperations."
    }

    $report.runningUpgradeLegacyProcessExited = $runningLegacyProcess.WaitForExit(5000)
    if (-not $report.runningUpgradeLegacyProcessExited) {
        throw "Windows Installer did not close the running legacy application during upgrade."
    }
    $runningLegacyProcess = $null

    $applicationPath = Wait-ApplicationInstalled -Directory $InstallDirectory
    $null = Assert-ApplicationVersion -ApplicationPath $applicationPath -ExpectedVersion $CurrentVersion
    if ((Get-Content -LiteralPath $sentinelPath -Raw).Trim() -ne $sentinelContent) {
        throw "The running-application MSI upgrade changed the AppData sentinel."
    }
    $report.runningUpgradeApplicationExitCode = Invoke-ApplicationSmoke -ApplicationPath $applicationPath -UserDataRoot $userDataRoot -TimeoutSeconds $StartupTimeoutSeconds

    $runningUninstallLog = Join-Path $installRoot "running-upgrade-uninstall.log"
    $runningUninstall = Invoke-MsiUninstall -ProductCode $currentProductCode -LogPath $runningUninstallLog
    $report.runningUpgradeUninstallExitCode = $runningUninstall.ExitCode
    Assert-SuccessCode -ExitCode $runningUninstall.ExitCode -Operation "Running-application current MSI uninstall"
    Wait-FileRemoved -Path $applicationPath
    $report.runningUpgradeVerified = $true
    $report.status = "passed"
}
catch {
    $failure = $_
    $report.status = "failed"
    $report.error = $_.Exception.Message
}
finally {
    if ($null -ne $runningLegacyProcess) {
        try {
            $runningLegacyProcess.Refresh()
            if (-not $runningLegacyProcess.HasExited) {
                Stop-Process -Id $runningLegacyProcess.Id -Force -ErrorAction SilentlyContinue
                $null = $runningLegacyProcess.WaitForExit(5000)
            }
        }
        catch {
            # The process may already have been closed by Windows Installer/Restart Manager.
        }
    }
    foreach ($productCode in @($currentProductCode, $rollbackProductCode, $legacyProductCode) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }) {
        if ((Get-MsiProductState -ProductCode $productCode) -ne -1) {
            $cleanup = Invoke-MsiUninstall -ProductCode $productCode
            if ($cleanup.ExitCode -notin @(0, 1605, 3010) -and $null -eq $failure) {
                $failure = [System.InvalidOperationException]::new("MSI cleanup failed with exit code $($cleanup.ExitCode)")
                $report.status = "failed"
                $report.error = $failure.Message
            }
        }
    }
    Wait-RememberedInstallDirectoryCleared -OriginalState $rememberedInstallDirectoryState
    Restore-RememberedInstallDirectory -State $rememberedInstallDirectoryState

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
