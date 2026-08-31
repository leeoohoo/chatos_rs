[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",

    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64",

    [string]$OutputDirectory = "artifacts\windows-native-acceptance",

    [switch]$BuildDesktop,

    [switch]$RunUiSmoke,

    [switch]$RequireAuthenticatedUi,

    [System.Management.Automation.PSCredential]$UiCredential,

    [switch]$PackageMsix
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "trx-evidence.ps1")
$isWindowsPlatform = if ($PSVersionTable.PSEdition -eq "Desktop") {
    $env:OS -eq "Windows_NT"
}
else {
    $IsWindows
}

if (-not $isWindowsPlatform) {
    throw "Windows native acceptance must run on Windows."
}
if ($RunUiSmoke -and -not $BuildDesktop) {
    throw "RunUiSmoke requires BuildDesktop so the tested executable is unambiguous."
}
if ($RequireAuthenticatedUi -and -not $RunUiSmoke) {
    throw "RequireAuthenticatedUi requires RunUiSmoke."
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

$evidenceRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    $OutputDirectory
}
else {
    Join-Path $repoRoot $OutputDirectory
}
$null = New-Item -ItemType Directory -Path $evidenceRoot -Force
$reportPath = Join-Path $evidenceRoot "acceptance-report.json"
$trxName = "windows-native-$Platform.trx"
$networkGuardTrxName = "networkguard-$Platform.trx"
foreach ($stalePath in @(
    $reportPath,
    (Join-Path $evidenceRoot $trxName),
    (Join-Path $evidenceRoot $networkGuardTrxName))) {
    Remove-Item $stalePath -Force -ErrorAction SilentlyContinue
}
$startedAt = [DateTimeOffset]::UtcNow
$passed = $false
$failureType = $null
$failureStage = $null
$executable = $null
$packages = @()
$nativeTestsPassed = $false
$networkGuardTestsPassed = $false
$desktopBuildPassed = $false
$uiSmokePassed = $false
$msixPackagePassed = $false
$nativeTestEvidence = $null
$networkGuardTestEvidence = $null
$currentStage = "windows_native_tests"
$expectedNativeTests = @(
    "NativeCommandExecutorCapturesOutputOnWindows",
    "NativeCommandTimeoutReclaimsChildProcessTreeOnWindows",
    "AppContainerEnforcesWorkspaceAclAndNetworkCapabilitiesOnWindows",
    "ConPtyAcceptsInputAndProducesOutputOnWindows",
    "ControlledCommandAcquiresLeaseBeforeSuspendedProcessCanRunOnWindows",
    "ControlledConPtyAcquiresLeaseBeforeSuspendedShellCanRunOnWindows",
    "ControlledConPtyAcquireFailureNeverResumesProcessOnWindows",
    "ControlledAppContainerProfileAndWorkspaceAclAreRemovedAfterUseOnWindows",
    "CredentialManagerRoundTripsAndDeletesRandomSecretOnWindows")

function Get-RelativeEvidencePath {
    param([Parameter(Mandatory)] [string]$Path)

    $root = [IO.Path]::GetFullPath($repoRoot).TrimEnd('\') + '\'
    $fullPath = [IO.Path]::GetFullPath($Path)
    if ($fullPath.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
        return $fullPath.Substring($root.Length).Replace('\', '/')
    }
    return [IO.Path]::GetFileName($fullPath)
}

function Get-EvidenceFile {
    param([Parameter(Mandatory)] [IO.FileInfo]$File)

    return [ordered]@{
        path = Get-RelativeEvidencePath $File.FullName
        length = $File.Length
        sha256 = (Get-FileHash -Path $File.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

Push-Location $repoRoot
try {
    dotnet test .\tests\ChatOS.Connector.Tests\ChatOS.Connector.Tests.csproj `
        -c $Configuration `
        --filter "Category=WindowsNative" `
        --logger "trx;LogFileName=$trxName" `
        --results-directory $evidenceRoot `
        --nologo
    if ($LASTEXITCODE -ne 0) {
        throw "Windows native tests failed."
    }
    $nativeTrxPath = Join-Path $evidenceRoot $trxName
    $nativeTestEvidence = Assert-TrxTestRun `
        -Path $nativeTrxPath `
        -EvidenceRoot $evidenceRoot `
        -ExpectedTestNames $expectedNativeTests `
        -MinimumTestCount $expectedNativeTests.Count
    $nativeTestsPassed = $true
    $currentStage = "networkguard_service_tests"
    dotnet test .\tests\ChatOS.NetworkGuard.Tests\ChatOS.NetworkGuard.Tests.csproj `
        -c $Configuration `
        --logger "trx;LogFileName=$networkGuardTrxName" `
        --results-directory $evidenceRoot `
        --nologo
    if ($LASTEXITCODE -ne 0) {
        throw "NetworkGuard service tests failed."
    }
    $networkGuardTrxPath = Join-Path $evidenceRoot $networkGuardTrxName
    $networkGuardTestEvidence = Assert-TrxTestRun `
        -Path $networkGuardTrxPath `
        -EvidenceRoot $evidenceRoot `
        -MinimumTestCount 19
    $networkGuardTestsPassed = $true

    if ($BuildDesktop) {
        $currentStage = "desktop_build"
        & (Join-Path $repoRoot "build\build.ps1") `
            -Configuration $Configuration `
            -Platform $Platform

        $runtimeIdentifier = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
        $outputRoot = Join-Path $repoRoot "src\ChatOS.Desktop\bin\$Platform\$Configuration"
        $executable = Get-ChildItem $outputRoot -Filter "ChatOS.Desktop.exe" -File -Recurse |
            Where-Object { $_.FullName -like "*$runtimeIdentifier*" } |
            Sort-Object LastWriteTimeUtc -Descending |
            Select-Object -First 1
        if (-not $executable) {
            throw "Desktop build did not produce the requested executable."
        }
        $desktopBuildPassed = $true
    }

    if ($RunUiSmoke) {
        $currentStage = "ui_smoke"
        $uiSmokeParameters = @{ ExecutablePath = $executable.FullName }
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
    }

    if ($PackageMsix) {
        $currentStage = "msix_package"
        & (Join-Path $repoRoot "build\package.ps1") -Platform $Platform
        $packages = @(Get-ChildItem `
            (Join-Path $repoRoot "src\ChatOS.Desktop\AppPackages\$Platform") `
            -File -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.Extension -in ".msix", ".msixbundle", ".appx", ".appxbundle" })
        if ($packages.Count -eq 0) {
            throw "MSIX acceptance did not produce a package."
        }
        $msixPackagePassed = $true
    }

    $passed = $true
}
catch {
    $failureType = $_.Exception.GetType().FullName
    $failureStage = $currentStage
}
finally {
    Pop-Location
    $operatingSystem = Get-CimInstance Win32_OperatingSystem
    $computerSystem = Get-CimInstance Win32_ComputerSystem
    $trx = Get-ChildItem $evidenceRoot -Filter $trxName -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    $networkGuardTrx = Get-ChildItem $evidenceRoot -Filter $networkGuardTrxName -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    $artifacts = @()
    if ($trx) { $artifacts += Get-EvidenceFile $trx }
    if ($networkGuardTrx) { $artifacts += Get-EvidenceFile $networkGuardTrx }
    if ($executable) { $artifacts += Get-EvidenceFile $executable }
    foreach ($package in $packages) { $artifacts += Get-EvidenceFile $package }

    $report = [ordered]@{
        schema_version = 3
        passed = $passed
        failure_type = $failureType
        failure_stage = $failureStage
        started_at = $startedAt.ToString("O")
        finished_at = [DateTimeOffset]::UtcNow.ToString("O")
        configuration = $Configuration
        platform = $Platform
        checks = [ordered]@{
            windows_native_tests = [ordered]@{ requested = $true; passed = $nativeTestsPassed }
            networkguard_service_tests = [ordered]@{ requested = $true; passed = $networkGuardTestsPassed }
            desktop_build = [ordered]@{ requested = [bool]$BuildDesktop; passed = $desktopBuildPassed }
            ui_smoke = [ordered]@{
                requested = [bool]$RunUiSmoke
                authenticated = [bool]$RequireAuthenticatedUi
                passed = $uiSmokePassed
            }
            msix_package = [ordered]@{ requested = [bool]$PackageMsix; passed = $msixPackagePassed }
        }
        test_runs = [ordered]@{
            windows_native = $nativeTestEvidence
            networkguard_service = $networkGuardTestEvidence
        }
        system = [ordered]@{
            caption = $operatingSystem.Caption
            version = $operatingSystem.Version
            build_number = $operatingSystem.BuildNumber
            os_architecture = $operatingSystem.OSArchitecture
            system_type = $computerSystem.SystemType
            process_architecture = [Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString()
        }
        artifacts = $artifacts
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content -Path $reportPath -Encoding utf8
}

if ($ownsUiCredential -and $UiCredential) { $UiCredential.Password.Dispose() }

if (-not $passed) {
    throw "Windows native acceptance failed. See $reportPath for the failure type and TRX evidence."
}

Write-Host "Windows native acceptance passed. Evidence: $reportPath"
