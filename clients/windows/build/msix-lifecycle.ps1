[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path $_ -PathType Leaf })]
    [string]$PackagePath,

    [ValidateScript({ -not $_ -or (Test-Path $_ -PathType Leaf) })]
    [string]$PreviousPackagePath,

    [string]$OutputDirectory = "artifacts\windows-msix-lifecycle",

    [switch]$AllowUnsigned,

    [switch]$RequireAuthenticatedUi,

    [System.Management.Automation.PSCredential]$UiCredential,

    [switch]$KeepInstalled
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$isWindowsPlatform = if ($PSVersionTable.PSEdition -eq "Desktop") {
    $env:OS -eq "Windows_NT"
}
else {
    $IsWindows
}

if (-not $isWindowsPlatform) {
    throw "MSIX lifecycle acceptance must run on Windows."
}
$ownsUiCredential = $false
$environmentUiUsername = [Environment]::GetEnvironmentVariable("CHATOS_UI_TEST_USERNAME", [EnvironmentVariableTarget]::Process)
$environmentUiPassword = [Environment]::GetEnvironmentVariable("CHATOS_UI_TEST_PASSWORD", [EnvironmentVariableTarget]::Process)
[Environment]::SetEnvironmentVariable("CHATOS_UI_TEST_USERNAME", $null, [EnvironmentVariableTarget]::Process)
[Environment]::SetEnvironmentVariable("CHATOS_UI_TEST_PASSWORD", $null, [EnvironmentVariableTarget]::Process)
if ($RequireAuthenticatedUi -and -not $UiCredential) {
    if ([string]::IsNullOrWhiteSpace($environmentUiUsername) -or [string]::IsNullOrWhiteSpace($environmentUiPassword)) {
        throw "Authenticated UI acceptance requires both CHATOS_UI_TEST_USERNAME and CHATOS_UI_TEST_PASSWORD."
    }
    $secureUiPassword = ConvertTo-SecureString $environmentUiPassword -AsPlainText -Force
    $UiCredential = [System.Management.Automation.PSCredential]::new($environmentUiUsername, $secureUiPassword)
    $ownsUiCredential = $true
}
$environmentUiUsername = $null
$environmentUiPassword = $null

$package = Get-Item (Resolve-Path $PackagePath).Path
$previousPackage = if ([string]::IsNullOrWhiteSpace($PreviousPackagePath)) {
    $null
}
else {
    Get-Item (Resolve-Path $PreviousPackagePath).Path
}
$evidenceRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory
}
else {
    Join-Path $repoRoot $OutputDirectory
}
$null = New-Item -ItemType Directory -Path $evidenceRoot -Force
$reportPath = Join-Path $evidenceRoot "msix-lifecycle-report.json"
$startedAt = [DateTimeOffset]::UtcNow
$passed = $false
$failureType = $null
$failureStage = $null
$installed = $null
$manifest = $null
$previousManifest = $null
$installationStarted = $false
$signatureValidated = $false
$previousInstallPassed = $false
$currentInstallPassed = $false
$uiSmokePassed = $false
$cleanupPassed = $false
$signatureStatus = $null
$previousSignatureStatus = $null
$currentStage = "package_validation"

function Read-MsixManifest {
    param([Parameter(Mandatory)] [IO.FileInfo]$File)

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($File.FullName)
    try {
        $entry = $archive.GetEntry("AppxManifest.xml")
        if (-not $entry) { $entry = $archive.GetEntry("Package.appxmanifest") }
        if (-not $entry) { throw "The package does not contain an Appx manifest." }
        $stream = $entry.Open()
        try {
            $reader = [IO.StreamReader]::new($stream)
            try { [xml]$xml = $reader.ReadToEnd() }
            finally { $reader.Dispose() }
        }
        finally {
            $stream.Dispose()
        }
    }
    finally {
        $archive.Dispose()
    }

    $identity = $xml.Package.Identity
    $application = @($xml.Package.Applications.Application)[0]
    if (-not $identity -or -not $application) {
        throw "The package manifest has no identity or application."
    }
    return [ordered]@{
        name = [string]$identity.Name
        publisher = [string]$identity.Publisher
        version = [version]$identity.Version
        architecture = [string]$identity.ProcessorArchitecture
        application_id = [string]$application.Id
    }
}

function Install-Msix {
    param([Parameter(Mandatory)] [IO.FileInfo]$File)

    $arguments = @{
        Path = $File.FullName
        ForceApplicationShutdown = $true
        ErrorAction = "Stop"
    }
    if ($AllowUnsigned) { $arguments.AllowUnsigned = $true }
    Add-AppxPackage @arguments
}

try {
    $manifest = Read-MsixManifest $package
    if ($previousPackage) {
        $previousManifest = Read-MsixManifest $previousPackage
        if ($previousManifest.name -ne $manifest.name -or
            $previousManifest.publisher -ne $manifest.publisher) {
            throw "Previous and current packages do not share the same identity."
        }
        if ($previousManifest.version -ge $manifest.version) {
            throw "The previous package version must be lower than the current package version."
        }
    }

    $signature = Get-AuthenticodeSignature $package.FullName
    $signatureStatus = $signature.Status.ToString()
    if (-not $AllowUnsigned -and
        $signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "The current package does not have a valid trusted signature."
    }
    if ($previousPackage) {
        $previousSignature = Get-AuthenticodeSignature $previousPackage.FullName
        $previousSignatureStatus = $previousSignature.Status.ToString()
        if (-not $AllowUnsigned -and
            $previousSignature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
            throw "The previous package does not have a valid trusted signature."
        }
    }
    $signatureValidated = $true

    $existing = @(Get-AppxPackage -Name $manifest.name -ErrorAction SilentlyContinue)
    if ($existing.Count -gt 0) {
        throw "The package is already installed for this Windows account. Use a disposable test account."
    }

    if ($previousPackage) {
        $currentStage = "previous_package_install"
        $installationStarted = $true
        Install-Msix $previousPackage
        $installedPrevious = Get-AppxPackage -Name $manifest.name -ErrorAction Stop
        if ([version]$installedPrevious.Version -ne $previousManifest.version) {
            throw "The previous package did not install with the expected version."
        }
        $previousInstallPassed = $true
    }

    $currentStage = "current_package_install"
    $installationStarted = $true
    Install-Msix $package
    $installed = Get-AppxPackage -Name $manifest.name -ErrorAction Stop
    if ([version]$installed.Version -ne $manifest.version) {
        throw "The installed package version does not match the current package."
    }
    $currentInstallPassed = $true

    $appUserModelId = "$($installed.PackageFamilyName)!$($manifest.application_id)"
    $currentStage = "ui_smoke"
    $uiSmokeParameters = @{ AppUserModelId = $appUserModelId }
    if ($RequireAuthenticatedUi) { $uiSmokeParameters.RequireAuthenticated = $true }
    if ($RequireAuthenticatedUi -and $UiCredential) {
        $env:CHATOS_UI_TEST_USERNAME = $UiCredential.UserName
        $env:CHATOS_UI_TEST_PASSWORD = $UiCredential.GetNetworkCredential().Password
    }
    try {
        & (Join-Path $repoRoot "build\ui-smoke.ps1") @uiSmokeParameters
    }
    finally {
        Remove-Item Env:CHATOS_UI_TEST_USERNAME -ErrorAction SilentlyContinue
        Remove-Item Env:CHATOS_UI_TEST_PASSWORD -ErrorAction SilentlyContinue
    }
    $uiSmokePassed = $true
    $passed = $true
}
catch {
    $failureType = $_.Exception.GetType().FullName
    $failureStage = $currentStage
}
finally {
    $currentStage = "package_cleanup"
    if ($installationStarted -and -not $KeepInstalled -and $manifest) {
        Get-Process -Name "ChatOS.Desktop" -ErrorAction SilentlyContinue |
            Stop-Process -Force -ErrorAction SilentlyContinue
        Get-AppxPackage -Name $manifest.name -ErrorAction SilentlyContinue |
            ForEach-Object {
                Remove-AppxPackage -Package $_.PackageFullName -ErrorAction SilentlyContinue
            }
    }

    $remaining = if ($manifest) {
        @(Get-AppxPackage -Name $manifest.name -ErrorAction SilentlyContinue).Count
    }
    else {
        0
    }
    $cleanupPassed = [bool]$KeepInstalled -or -not $installationStarted -or $remaining -eq 0
    if ($passed -and -not $cleanupPassed) {
        $passed = $false
        $failureType = "ChatOS.Acceptance.PackageCleanupFailed"
        $failureStage = $currentStage
    }
    $report = [ordered]@{
        schema_version = 2
        passed = $passed
        failure_type = $failureType
        failure_stage = $failureStage
        started_at = $startedAt.ToString("O")
        finished_at = [DateTimeOffset]::UtcNow.ToString("O")
        identity = if ($manifest) {
            [ordered]@{
                name = $manifest.name
                publisher = $manifest.publisher
                version = $manifest.version.ToString()
                architecture = $manifest.architecture
                application_id = $manifest.application_id
            }
        } else { $null }
        previous_version = if ($previousManifest) { $previousManifest.version.ToString() } else { $null }
        allow_unsigned = [bool]$AllowUnsigned
        signature_status = $signatureStatus
        previous_signature_status = $previousSignatureStatus
        kept_installed = [bool]$KeepInstalled
        remaining_package_count = $remaining
        checks = [ordered]@{
            signature_policy = [ordered]@{ requested = $true; passed = $signatureValidated }
            previous_package_install = [ordered]@{ requested = $null -ne $previousPackage; passed = $previousInstallPassed }
            current_package_install = [ordered]@{ requested = $true; passed = $currentInstallPassed }
            ui_smoke = [ordered]@{
                requested = $true
                authenticated = [bool]$RequireAuthenticatedUi
                passed = $uiSmokePassed
            }
            cleanup = [ordered]@{ requested = -not [bool]$KeepInstalled; passed = $cleanupPassed }
        }
        package = [ordered]@{
            file_name = $package.Name
            length = $package.Length
            sha256 = (Get-FileHash $package.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
        previous_package = if ($previousPackage) {
            [ordered]@{
                file_name = $previousPackage.Name
                length = $previousPackage.Length
                sha256 = (Get-FileHash $previousPackage.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        } else { $null }
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -Path $reportPath -Encoding utf8
}

if ($ownsUiCredential -and $UiCredential) { $UiCredential.Password.Dispose() }

if (-not $passed) {
    throw "MSIX lifecycle acceptance failed. See $reportPath for the failure type."
}
if (-not $KeepInstalled -and $remaining -ne 0) {
    throw "MSIX lifecycle completed but the test package is still installed."
}

Write-Host "MSIX lifecycle acceptance passed. Evidence: $reportPath"
