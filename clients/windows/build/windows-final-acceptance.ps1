[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet("x64", "ARM64")]
    [string]$RuntimePlatform,

    [ValidateScript({ -not $_ -or (Test-Path $_ -PathType Leaf) })]
    [string]$PackagePath,

    [ValidateScript({ -not $_ -or (Test-Path $_ -PathType Leaf) })]
    [string]$PreviousPackagePath,

    [string]$NetworkGuardCertificateThumbprint,

    [ValidateScript({ -not $_ -or (Test-Path $_ -PathType Container) })]
    [string]$NetworkGuardX64PackageDirectory,

    [ValidateScript({ -not $_ -or (Test-Path $_ -PathType Container) })]
    [string]$NetworkGuardArm64PackageDirectory,

    [string]$MsixCertificateThumbprint,

    [string]$Ipv6Literal,

    [string]$OutputDirectory = "artifacts\windows-final-acceptance",

    [switch]$RequireAuthenticatedUi,

    [switch]$RequireUpgrade,

    [switch]$AllowUnsignedMsix,

    [switch]$AllowUnsignedDriver,

    [switch]$AllowTestSignedDriver,

    [switch]$BuildCurrentPackage,

    [Parameter(Mandatory)]
    [switch]$ConfirmDisposableMachine
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows) { throw "Final Windows acceptance must run on Windows." }
if (-not $ConfirmDisposableMachine) {
    throw "Run only on a disposable Windows acceptance machine and pass ConfirmDisposableMachine."
}
if ($RequireUpgrade -and [string]::IsNullOrWhiteSpace($PreviousPackagePath)) {
    throw "RequireUpgrade requires PreviousPackagePath."
}
if ($BuildCurrentPackage -and -not [string]::IsNullOrWhiteSpace($PackagePath)) {
    throw "BuildCurrentPackage and PackagePath are mutually exclusive."
}
if (-not $BuildCurrentPackage -and [string]::IsNullOrWhiteSpace($PackagePath)) {
    throw "PackagePath is required unless BuildCurrentPackage is selected."
}
if (-not [string]::IsNullOrWhiteSpace($NetworkGuardCertificateThumbprint)) {
    $NetworkGuardCertificateThumbprint = $NetworkGuardCertificateThumbprint.Replace(" ", "").ToUpperInvariant()
    if ($NetworkGuardCertificateThumbprint -notmatch '^[A-F0-9]{40}$') {
        throw "NetworkGuardCertificateThumbprint must be a 40-character SHA-1 certificate thumbprint."
    }
}
if (-not [string]::IsNullOrWhiteSpace($MsixCertificateThumbprint)) {
    $MsixCertificateThumbprint = $MsixCertificateThumbprint.Replace(" ", "").ToUpperInvariant()
    if ($MsixCertificateThumbprint -notmatch '^[A-F0-9]{40}$') {
        throw "MsixCertificateThumbprint must be a 40-character SHA-1 certificate thumbprint."
    }
}
if ($BuildCurrentPackage -and -not $AllowUnsignedMsix -and [string]::IsNullOrWhiteSpace($MsixCertificateThumbprint)) {
    throw "Building the current release package requires MsixCertificateThumbprint unless AllowUnsignedMsix is selected."
}
$hasX64NetworkGuardPackage = -not [string]::IsNullOrWhiteSpace($NetworkGuardX64PackageDirectory)
$hasArm64NetworkGuardPackage = -not [string]::IsNullOrWhiteSpace($NetworkGuardArm64PackageDirectory)
if ($hasX64NetworkGuardPackage -xor $hasArm64NetworkGuardPackage) {
    throw "Provide both NetworkGuardX64PackageDirectory and NetworkGuardArm64PackageDirectory."
}
if ($AllowUnsignedDriver -and $AllowTestSignedDriver) {
    throw "AllowUnsignedDriver and AllowTestSignedDriver are mutually exclusive."
}
if ($hasX64NetworkGuardPackage -and -not [string]::IsNullOrWhiteSpace($NetworkGuardCertificateThumbprint)) {
    throw "Existing NetworkGuard package directories and NetworkGuardCertificateThumbprint are mutually exclusive."
}
if (-not $hasX64NetworkGuardPackage) {
    if (-not [string]::IsNullOrWhiteSpace($NetworkGuardCertificateThumbprint) -and -not $AllowTestSignedDriver) {
        throw "A locally signed NetworkGuard build is test-only; pass AllowTestSignedDriver explicitly."
    }
    if ([string]::IsNullOrWhiteSpace($NetworkGuardCertificateThumbprint) -and -not $AllowUnsignedDriver) {
        throw "Formal acceptance requires both Microsoft production-signed NetworkGuard package directories. Local unsigned builds require AllowUnsignedDriver."
    }
}

$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Run final Windows acceptance from an elevated PowerShell window."
}
$osArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ($RuntimePlatform -eq "ARM64" -and $osArchitecture -ne "Arm64") {
    throw "ARM64 final acceptance must run on native ARM64 Windows."
}
if ($RuntimePlatform -eq "x64" -and $osArchitecture -ne "X64") {
    throw "x64 final acceptance must run on x64 Windows."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$evidenceRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
}
else {
    [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDirectory))
}
$runId = "{0}-{1}" -f `
    [DateTimeOffset]::UtcNow.ToString("yyyyMMdd-HHmmss"), `
    [Guid]::NewGuid().ToString("N").Substring(0, 8)
$runRoot = Join-Path $evidenceRoot "$RuntimePlatform-$runId"
$nativeRoot = Join-Path $runRoot "native"
$msixRoot = Join-Path $runRoot "msix"
$networkGuardRoot = Join-Path $runRoot "networkguard"
$deliverablesRoot = Join-Path $runRoot "deliverables"
$null = New-Item -ItemType Directory -Path $nativeRoot -Force
$null = New-Item -ItemType Directory -Path $msixRoot -Force
$null = New-Item -ItemType Directory -Path $networkGuardRoot -Force
$null = New-Item -ItemType Directory -Path $deliverablesRoot -Force
$orchestrationReportPath = Join-Path $runRoot "orchestration-report.json"
$verificationReportPath = Join-Path $runRoot "verification-report.json"
$startedAt = [DateTimeOffset]::UtcNow
$currentStage = "wdk_x64_build"
$failureType = $null
$failureStage = $null
$passed = $false
$uiCredential = $null
$wdkX64Passed = $false
$wdkArm64Passed = $false
$msixPackageBuildPassed = $false
$nativePassed = $false
$msixPassed = $false
$networkGuardPassed = $false
$verificationPassed = $false
$resolvedPackagePath = $null
$resolvedPreviousPackagePath = $null

$uiUsername = [Environment]::GetEnvironmentVariable("CHATOS_UI_TEST_USERNAME", [EnvironmentVariableTarget]::Process)
$uiPassword = [Environment]::GetEnvironmentVariable("CHATOS_UI_TEST_PASSWORD", [EnvironmentVariableTarget]::Process)
[Environment]::SetEnvironmentVariable("CHATOS_UI_TEST_USERNAME", $null, [EnvironmentVariableTarget]::Process)
[Environment]::SetEnvironmentVariable("CHATOS_UI_TEST_PASSWORD", $null, [EnvironmentVariableTarget]::Process)
if ($RequireAuthenticatedUi) {
    if ([string]::IsNullOrWhiteSpace($uiUsername) -or [string]::IsNullOrWhiteSpace($uiPassword)) {
        throw "RequireAuthenticatedUi requires CHATOS_UI_TEST_USERNAME and CHATOS_UI_TEST_PASSWORD."
    }
    $securePassword = ConvertTo-SecureString $uiPassword -AsPlainText -Force
    $uiCredential = [System.Management.Automation.PSCredential]::new($uiUsername, $securePassword)
}
$uiUsername = $null
$uiPassword = $null

function Get-ReportReference {
    param([string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path) -or -not (Test-Path $Path -PathType Leaf)) {
        return $null
    }
    $file = Get-Item $Path
    return [ordered]@{
        file_name = $file.Name
        length = $file.Length
        sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

Push-Location $repoRoot
try {
    $wdkX64Package = Join-Path $deliverablesRoot "networkguard-x64"
    $null = New-Item -ItemType Directory -Path $wdkX64Package -Force
    $wdkArm64Package = Join-Path $deliverablesRoot "networkguard-ARM64"
    $null = New-Item -ItemType Directory -Path $wdkArm64Package -Force

    if ($hasX64NetworkGuardPackage) {
        $currentStage = "networkguard_x64_package_copy"
        Copy-Item (Join-Path (Resolve-Path $NetworkGuardX64PackageDirectory).Path "*") $wdkX64Package -Recurse -Force
        $wdkX64Passed = $true
        $currentStage = "networkguard_arm64_package_copy"
        Copy-Item (Join-Path (Resolve-Path $NetworkGuardArm64PackageDirectory).Path "*") $wdkArm64Package -Recurse -Force
        $wdkArm64Passed = $true
    }
    else {
        $currentStage = "wdk_x64_build"
        $driverBuildParameters = @{ Configuration = "Release"; Platform = "x64" }
        if (-not [string]::IsNullOrWhiteSpace($NetworkGuardCertificateThumbprint)) {
            $driverBuildParameters.CertificateThumbprint = $NetworkGuardCertificateThumbprint
        }
        & .\build\networkguard.ps1 @driverBuildParameters
        $wdkX64Source = Join-Path $repoRoot "artifacts\networkguard\x64"
        Copy-Item (Join-Path $wdkX64Source "*") $wdkX64Package -Recurse -Force
        $wdkX64Passed = $true

        $currentStage = "wdk_arm64_build"
        $driverBuildParameters.Platform = "ARM64"
        & .\build\networkguard.ps1 @driverBuildParameters
        $wdkArm64Source = Join-Path $repoRoot "artifacts\networkguard\ARM64"
        Copy-Item (Join-Path $wdkArm64Source "*") $wdkArm64Package -Recurse -Force
        $wdkArm64Passed = $true
    }
    $wdkX64Report = Join-Path $wdkX64Package "build-report.json"
    if (-not (Test-Path $wdkX64Report -PathType Leaf)) { throw "x64 NetworkGuard build report was not provided." }
    $wdkArm64Report = Join-Path $wdkArm64Package "build-report.json"
    if (-not (Test-Path $wdkArm64Report -PathType Leaf)) { throw "ARM64 NetworkGuard build report was not provided." }

    $currentStage = "msix_package_build"
    if ($BuildCurrentPackage) {
        $packageParameters = @{ Platform = $RuntimePlatform }
        if (-not [string]::IsNullOrWhiteSpace($MsixCertificateThumbprint)) {
            $packageParameters.CertificateThumbprint = $MsixCertificateThumbprint
        }
        & .\build\package.ps1 @packageParameters
        $resolvedPackage = Get-ChildItem `
            (Join-Path $repoRoot "src\ChatOS.Desktop\AppPackages\$RuntimePlatform") `
            -File -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.Extension -in ".msix", ".msixbundle", ".appx", ".appxbundle" } |
            Sort-Object LastWriteTimeUtc -Descending |
            Select-Object -First 1
        if (-not $resolvedPackage) { throw "Current MSIX build did not produce an installable package." }
        $resolvedPackagePath = $resolvedPackage.FullName
        $msixPackageBuildPassed = $true
    }
    else {
        $resolvedPackagePath = (Resolve-Path $PackagePath).Path
    }
    $msixDeliverables = Join-Path $deliverablesRoot "msix"
    $currentMsixDeliverables = Join-Path $msixDeliverables "current"
    $null = New-Item -ItemType Directory -Path $currentMsixDeliverables -Force
    $copiedPackagePath = Join-Path $currentMsixDeliverables ([IO.Path]::GetFileName($resolvedPackagePath))
    Copy-Item $resolvedPackagePath $copiedPackagePath -Force
    $resolvedPackagePath = $copiedPackagePath
    if (-not [string]::IsNullOrWhiteSpace($PreviousPackagePath)) {
        $previousMsixDeliverables = Join-Path $msixDeliverables "previous"
        $null = New-Item -ItemType Directory -Path $previousMsixDeliverables -Force
        $resolvedPreviousPackagePath = Join-Path $previousMsixDeliverables ([IO.Path]::GetFileName($PreviousPackagePath))
        Copy-Item (Resolve-Path $PreviousPackagePath).Path $resolvedPreviousPackagePath -Force
    }
    $runtimeNetworkGuardPackage = if ($RuntimePlatform -eq "ARM64") { $wdkArm64Package } else { $wdkX64Package }

    $currentStage = "native_acceptance"
    $nativeParameters = @{
        Configuration = "Release"
        Platform = $RuntimePlatform
        OutputDirectory = $nativeRoot
        BuildDesktop = $true
        RunUiSmoke = $true
    }
    if ($RequireAuthenticatedUi) {
        $nativeParameters.RequireAuthenticatedUi = $true
        $nativeParameters.UiCredential = $uiCredential
    }
    & .\build\native-acceptance.ps1 @nativeParameters
    $nativeReport = Join-Path $nativeRoot "acceptance-report.json"
    if (-not (Test-Path $nativeReport -PathType Leaf)) { throw "Native acceptance report was not produced." }
    $nativePassed = $true

    $currentStage = "msix_lifecycle"
    $msixParameters = @{
        PackagePath = $resolvedPackagePath
        OutputDirectory = $msixRoot
    }
    if (-not [string]::IsNullOrWhiteSpace($resolvedPreviousPackagePath)) {
        $msixParameters.PreviousPackagePath = $resolvedPreviousPackagePath
    }
    if ($AllowUnsignedMsix) { $msixParameters.AllowUnsigned = $true }
    if ($RequireAuthenticatedUi) {
        $msixParameters.RequireAuthenticatedUi = $true
        $msixParameters.UiCredential = $uiCredential
    }
    & .\build\msix-lifecycle.ps1 @msixParameters
    $msixReport = Join-Path $msixRoot "msix-lifecycle-report.json"
    if (-not (Test-Path $msixReport -PathType Leaf)) { throw "MSIX lifecycle report was not produced." }
    $msixPassed = $true

    $currentStage = "networkguard_acceptance"
    $networkGuardParameters = @{
        Configuration = "Release"
        Platform = $RuntimePlatform
        PackageDirectory = $runtimeNetworkGuardPackage
        OutputDirectory = $networkGuardRoot
        Disruptive = $true
        UninstallAfter = $true
    }
    if (-not [string]::IsNullOrWhiteSpace($Ipv6Literal)) {
        $networkGuardParameters.Ipv6Literal = $Ipv6Literal
    }
    & .\build\networkguard-acceptance.ps1 @networkGuardParameters
    $networkGuardReport = Get-ChildItem $networkGuardRoot -Filter acceptance-report.json -File -Recurse |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if (-not $networkGuardReport) { throw "NetworkGuard acceptance report was not produced." }
    $networkGuardReport = $networkGuardReport.FullName
    $networkGuardPassed = $true

    $currentStage = "evidence_verification"
    $verificationParameters = @{
        RuntimePlatform = $RuntimePlatform
        MsixPackage = $resolvedPackagePath
        NetworkGuardPackageDirectory = $runtimeNetworkGuardPackage
        NativeReport = $nativeReport
        MsixReport = $msixReport
        NetworkGuardReport = $networkGuardReport
        WdkX64Report = $wdkX64Report
        WdkArm64Report = $wdkArm64Report
        OutputPath = $verificationReportPath
    }
    if (-not [string]::IsNullOrWhiteSpace($resolvedPreviousPackagePath)) {
        $verificationParameters.PreviousMsixPackage = $resolvedPreviousPackagePath
    }
    if ($RequireAuthenticatedUi) { $verificationParameters.RequireAuthenticatedUi = $true }
    if ($RequireUpgrade) { $verificationParameters.RequireUpgrade = $true }
    if (-not [string]::IsNullOrWhiteSpace($Ipv6Literal)) { $verificationParameters.RequireIpv6Literal = $true }
    if ($AllowUnsignedMsix) { $verificationParameters.AllowUnsignedMsix = $true }
    if ($AllowUnsignedDriver) { $verificationParameters.AllowUnsignedDriver = $true }
    if ($AllowTestSignedDriver) { $verificationParameters.AllowTestSignedDriver = $true }
    & .\build\verify-windows-acceptance.ps1 @verificationParameters
    $verificationPassed = $true
    $passed = $true
}
catch {
    $failureType = $_.Exception.GetType().FullName
    $failureStage = $currentStage
}
finally {
    Pop-Location
    if ($uiCredential) { $uiCredential.Password.Dispose() }
    $report = [ordered]@{
        schema_version = 1
        passed = $passed
        failure_type = $failureType
        failure_stage = $failureStage
        started_at = $startedAt.ToString("O")
        finished_at = [DateTimeOffset]::UtcNow.ToString("O")
        runtime_platform = $RuntimePlatform
        os_architecture = $osArchitecture
        requirements = [ordered]@{
            authenticated_ui = [bool]$RequireAuthenticatedUi
            upgrade = [bool]$RequireUpgrade
            ipv6_literal = -not [string]::IsNullOrWhiteSpace($Ipv6Literal)
            unsigned_msix_allowed = [bool]$AllowUnsignedMsix
            unsigned_driver_allowed = [bool]$AllowUnsignedDriver
            test_signed_driver_allowed = [bool]$AllowTestSignedDriver
            production_driver_packages_supplied = $hasX64NetworkGuardPackage
            current_package_built = [bool]$BuildCurrentPackage
        }
        checks = [ordered]@{
            wdk_x64 = $wdkX64Passed
            wdk_arm64 = $wdkArm64Passed
            msix_package_build = [ordered]@{ requested = [bool]$BuildCurrentPackage; passed = $msixPackageBuildPassed }
            native = $nativePassed
            msix = $msixPassed
            networkguard = $networkGuardPassed
            verification = $verificationPassed
        }
        evidence = [ordered]@{
            current_package = Get-ReportReference $resolvedPackagePath
            previous_package = Get-ReportReference $resolvedPreviousPackagePath
            native = Get-ReportReference $nativeReport
            msix = Get-ReportReference $msixReport
            networkguard = Get-ReportReference $networkGuardReport
            wdk_x64 = Get-ReportReference $wdkX64Report
            wdk_arm64 = Get-ReportReference $wdkArm64Report
            verification = Get-ReportReference $verificationReportPath
        }
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content $orchestrationReportPath -Encoding utf8
}

if (-not $passed) {
    throw "Final Windows acceptance failed. See $orchestrationReportPath."
}

Write-Host "Final Windows acceptance passed. Evidence root: $runRoot"
