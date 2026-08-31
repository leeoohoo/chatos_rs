[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "trx-evidence.ps1")
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) "chatos-acceptance-verifier-$([Guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $fixtureRoot -Force

function Write-Fixture {
    param(
        [Parameter(Mandatory)] [string]$Name,
        [Parameter(Mandatory)] [object]$Value
    )
    $path = Join-Path $fixtureRoot $Name
    $parent = Split-Path $path -Parent
    $null = New-Item -ItemType Directory -Path $parent -Force
    $Value | ConvertTo-Json -Depth 12 | Set-Content $path -Encoding utf8
    return $path
}

function New-PassedCheck([bool]$Requested = $true) {
    return [ordered]@{ requested = $Requested; passed = $true }
}

function New-FileEvidence {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string]$RelativePath
    )
    $file = Get-Item $Path
    return [ordered]@{
        path = $RelativePath
        length = $file.Length
        sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function New-TrxFixture {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string[]]$Names
    )
    $results = foreach ($name in $Names) {
        $escaped = [System.Security.SecurityElement]::Escape($name)
        '<UnitTestResult testName="{0}" outcome="Passed" />' -f $escaped
    }
    $xml = @"
<?xml version="1.0" encoding="utf-8"?>
<TestRun xmlns="http://microsoft.com/schemas/VisualStudio/TeamTest/2010">
  <Results>$($results -join '')</Results>
  <ResultSummary outcome="Completed">
    <Counters total="$($Names.Count)" executed="$($Names.Count)" passed="$($Names.Count)" failed="0" />
  </ResultSummary>
</TestRun>
"@
    [IO.File]::WriteAllText($Path, $xml)
}

try {
    $msixPackagePath = Join-Path $fixtureRoot "ChatOS.msix"
    [IO.File]::WriteAllBytes($msixPackagePath, [byte[]](1, 2, 3, 4))
    $networkGuardPackage = Join-Path $fixtureRoot "networkguard-package"
    $networkGuardDriverRoot = Join-Path $networkGuardPackage "driver"
    $null = New-Item -ItemType Directory -Path $networkGuardDriverRoot -Force
    $networkGuardDriver = Join-Path $networkGuardDriverRoot "ChatOS.NetworkGuard.Driver.sys"
    $networkGuardCatalog = Join-Path $networkGuardDriverRoot "ChatOS.NetworkGuard.Driver.cat"
    $networkGuardManifest = Join-Path $networkGuardPackage "manifest.json"
    [IO.File]::WriteAllBytes($networkGuardDriver, [byte[]](10, 11, 12))
    [IO.File]::WriteAllBytes($networkGuardCatalog, [byte[]](20, 21, 22))
    [IO.File]::WriteAllBytes($networkGuardManifest, [byte[]](30, 31, 32))

    $nativeExpectedNames = @(
        "NativeCommandExecutorCapturesOutputOnWindows",
        "NativeCommandTimeoutReclaimsChildProcessTreeOnWindows",
        "AppContainerEnforcesWorkspaceAclAndNetworkCapabilitiesOnWindows",
        "ConPtyAcceptsInputAndProducesOutputOnWindows",
        "ControlledCommandAcquiresLeaseBeforeSuspendedProcessCanRunOnWindows",
        "ControlledConPtyAcquiresLeaseBeforeSuspendedShellCanRunOnWindows",
        "ControlledConPtyAcquireFailureNeverResumesProcessOnWindows",
        "ControlledAppContainerProfileAndWorkspaceAclAreRemovedAfterUseOnWindows",
        "CredentialManagerRoundTripsAndDeletesRandomSecretOnWindows")
    $nativeTrxPath = Join-Path $fixtureRoot "windows-native.trx"
    New-TrxFixture $nativeTrxPath @($nativeExpectedNames | ForEach-Object { "ChatOS.Connector.Tests.WindowsNativeAcceptanceTests.$_" })
    $nativeTrxEvidence = Assert-TrxTestRun `
        -Path $nativeTrxPath `
        -EvidenceRoot $fixtureRoot `
        -ExpectedTestNames $nativeExpectedNames `
        -MinimumTestCount 9

    $serviceTestNames = 1..19 | ForEach-Object { "ChatOS.NetworkGuard.Tests.ServiceAcceptanceTest$_" }
    $serviceTrxPath = Join-Path $fixtureRoot "networkguard-service.trx"
    New-TrxFixture $serviceTrxPath $serviceTestNames
    $serviceTrxEvidence = Assert-TrxTestRun `
        -Path $serviceTrxPath `
        -EvidenceRoot $fixtureRoot `
        -MinimumTestCount 19

    $e2eExpectedNames = @(
        "SignedPolicyAllowsOnlyApprovedHttpTlsAndLeavesNoLeaseResidue",
        "ServiceAndDriverRestartRemainFailClosedAndReconcileResidue")
    $e2eTrxPath = Join-Path $fixtureRoot "networkguard-e2e.trx"
    New-TrxFixture $e2eTrxPath @($e2eExpectedNames | ForEach-Object { "ChatOS.Connector.Tests.NetworkGuardEndToEndAcceptanceTests.$_" })
    $e2eTrxEvidence = Assert-TrxTestRun `
        -Path $e2eTrxPath `
        -EvidenceRoot $fixtureRoot `
        -ExpectedTestNames $e2eExpectedNames `
        -MinimumTestCount 2

    $nativePath = Write-Fixture "native.json" ([ordered]@{
        schema_version = 3
        passed = $true
        platform = "x64"
        checks = [ordered]@{
            windows_native_tests = (New-PassedCheck)
            networkguard_service_tests = (New-PassedCheck)
            desktop_build = (New-PassedCheck)
            ui_smoke = [ordered]@{ requested = $true; authenticated = $false; passed = $true }
        }
        test_runs = [ordered]@{
            windows_native = $nativeTrxEvidence
            networkguard_service = $serviceTrxEvidence
        }
    })
    $msixPath = Write-Fixture "msix.json" ([ordered]@{
        schema_version = 2
        passed = $true
        identity = [ordered]@{ architecture = "x64" }
        signature_status = "Valid"
        previous_signature_status = $null
        kept_installed = $false
        remaining_package_count = 0
        checks = [ordered]@{
            signature_policy = (New-PassedCheck)
            previous_package_install = [ordered]@{ requested = $false; passed = $false }
            current_package_install = (New-PassedCheck)
            ui_smoke = [ordered]@{ requested = $true; authenticated = $false; passed = $true }
            cleanup = (New-PassedCheck)
        }
        package = New-FileEvidence $msixPackagePath "ChatOS.msix"
        previous_package = $null
    })
    $networkGuardChecks = [ordered]@{}
    foreach ($name in @(
        "package_validation",
        "install",
        "signed_policy",
        "allowed_http_tls",
        "same_ip_denied_sni",
        "ip_literals_fail_closed",
        "dns_doh_quic_udp_fail_closed",
        "no_sni_fail_closed",
        "child_process_fail_closed",
        "service_driver_restart_fail_closed",
        "lease_residue_zero",
        "status_capture",
        "uninstall")) {
        $networkGuardChecks[$name] = (New-PassedCheck)
    }
    $networkGuardDriverEvidence = New-FileEvidence $networkGuardDriver "driver/ChatOS.NetworkGuard.Driver.sys"
    $networkGuardDriverEvidence.signature_status = "Valid"
    $networkGuardDriverEvidence.signer_thumbprint = "ABC"
    $networkGuardDriverEvidence.signer_subject = "CN=Microsoft Windows Hardware Compatibility Publisher"
    $networkGuardDriverEvidence.timestamp_subject = "CN=Microsoft Time-Stamp Service"
    $networkGuardCatalogEvidence = New-FileEvidence $networkGuardCatalog "driver/ChatOS.NetworkGuard.Driver.cat"
    $networkGuardCatalogEvidence.signature_status = "Valid"
    $networkGuardCatalogEvidence.signer_thumbprint = "ABC"
    $networkGuardCatalogEvidence.signer_subject = "CN=Microsoft Windows Hardware Compatibility Publisher"
    $networkGuardCatalogEvidence.timestamp_subject = "CN=Microsoft Time-Stamp Service"
    $networkGuardPath = Write-Fixture "networkguard.json" ([ordered]@{
        schema_version = 3
        passed = $true
        platform = "x64"
        disruptive_restart_tests = $true
        ipv6_literal_tested = $false
        test_run = $e2eTrxEvidence
        package = [ordered]@{
            signing_mode = "microsoft_production"
            driver = $networkGuardDriverEvidence
            catalog = $networkGuardCatalogEvidence
            manifest = New-FileEvidence $networkGuardManifest "manifest.json"
        }
        checks = $networkGuardChecks
    })
    function New-WdkFixture([string]$DirectoryName, [string]$Platform) {
        $root = Join-Path $fixtureRoot $DirectoryName
        $driverRoot = Join-Path $root "driver"
        $serviceRoot = Join-Path $root "service"
        $null = New-Item -ItemType Directory -Path $driverRoot -Force
        $null = New-Item -ItemType Directory -Path $serviceRoot -Force
        $files = [ordered]@{
            "driver/ChatOS.NetworkGuard.Driver.sys" = [byte[]](40, 41)
            "driver/ChatOS.NetworkGuard.Driver.cat" = [byte[]](42, 43)
            "driver/ChatOS.NetworkGuard.Driver.inf" = [byte[]](44, 45)
            "service/ChatOS.NetworkGuard.Service.exe" = [byte[]](46, 47)
        }
        $artifacts = @()
        foreach ($entry in $files.GetEnumerator()) {
            $path = Join-Path $root $entry.Key
            [IO.File]::WriteAllBytes($path, $entry.Value)
            $artifacts += New-FileEvidence $path $entry.Key
        }
        return Write-Fixture "$DirectoryName/build-report.json" ([ordered]@{
            schema_version = 2
            passed = $true
            platform = $Platform
            configuration = "Release"
            wdk_version = "10.0.26100.0"
            signed = $true
            signing_mode = "microsoft_production"
            driver_signature_status = "Valid"
            catalog_signature_status = "Valid"
            driver_signer_subject = "CN=Microsoft Windows Hardware Compatibility Publisher"
            catalog_signer_subject = "CN=Microsoft Windows Hardware Compatibility Publisher"
            artifacts = $artifacts
        })
    }
    $wdkX64Path = New-WdkFixture "wdk-x64" "x64"
    $wdkArm64Path = New-WdkFixture "wdk-arm64" "ARM64"

    $parameters = @{
        RuntimePlatform = "x64"
        MsixPackage = $msixPackagePath
        NetworkGuardPackageDirectory = $networkGuardPackage
        NativeReport = $nativePath
        MsixReport = $msixPath
        NetworkGuardReport = $networkGuardPath
        WdkX64Report = $wdkX64Path
        WdkArm64Report = $wdkArm64Path
        OutputPath = (Join-Path $fixtureRoot "success-summary.json")
    }
    try {
        & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    }
    catch {
        if (Test-Path $parameters.OutputPath -PathType Leaf) {
            Write-Error (Get-Content $parameters.OutputPath -Raw)
        }
        throw
    }
    $success = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($success.passed -ne $true -or @($success.failures).Count -ne 0) {
        throw "The valid acceptance fixture did not pass verification."
    }

    $parameters.OutputPath = Join-Path $fixtureRoot "failure-summary.json"
    $parameters.RequireUpgrade = $true
    $failureRaised = $false
    try {
        & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    }
    catch {
        $failureRaised = $true
    }
    if (-not $failureRaised) { throw "The missing-upgrade fixture unexpectedly passed verification." }
    $failure = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($failure.passed -ne $false -or @($failure.failures) -notcontains "msix.upgrade_not_proven") {
        throw "The missing-upgrade fixture did not record the expected evidence failure."
    }

    $parameters.Remove("RequireUpgrade")
    $parameters.OutputPath = Join-Path $fixtureRoot "tampered-package-summary.json"
    [IO.File]::WriteAllBytes($msixPackagePath, [byte[]](1, 2, 3, 4, 5))
    $tamperFailureRaised = $false
    try {
        & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    }
    catch {
        $tamperFailureRaised = $true
    }
    if (-not $tamperFailureRaised) { throw "The tampered MSIX fixture unexpectedly passed verification." }
    $tamperFailure = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($tamperFailure.passed -ne $false -or
        (@($tamperFailure.failures) -notcontains "msix.package.length" -and
         @($tamperFailure.failures) -notcontains "msix.package.sha256")) {
        throw "The tampered MSIX fixture did not record an artifact integrity failure."
    }

    [IO.File]::WriteAllBytes($msixPackagePath, [byte[]](1, 2, 3, 4))
    $previousMsixPackagePath = Join-Path $fixtureRoot "ChatOS-previous.msix"
    [IO.File]::WriteAllBytes($previousMsixPackagePath, [byte[]](5, 6, 7))
    $upgradeReport = Get-Content $msixPath -Raw | ConvertFrom-Json
    $upgradeReport.previous_signature_status = "Valid"
    $upgradeReport.checks.previous_package_install.requested = $true
    $upgradeReport.checks.previous_package_install.passed = $true
    $upgradeReport.previous_package = New-FileEvidence $previousMsixPackagePath "ChatOS-previous.msix"
    $upgradeReportPath = Write-Fixture "msix-upgrade.json" $upgradeReport
    $parameters.MsixReport = $upgradeReportPath
    $parameters.PreviousMsixPackage = $previousMsixPackagePath
    $parameters.RequireUpgrade = $true
    $parameters.OutputPath = Join-Path $fixtureRoot "upgrade-summary.json"
    & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    $upgrade = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($upgrade.passed -ne $true -or @($upgrade.failures).Count -ne 0) {
        throw "The valid upgrade fixture did not pass verification."
    }

    $productionNetworkGuardJson = Get-Content $networkGuardPath -Raw
    $productionWdkX64Json = Get-Content $wdkX64Path -Raw
    $productionWdkArm64Json = Get-Content $wdkArm64Path -Raw
    $localNetworkGuard = $productionNetworkGuardJson | ConvertFrom-Json
    $localNetworkGuard.package.signing_mode = "local_test"
    $localNetworkGuard.package.driver.signer_subject = "CN=ChatOS Test Driver"
    $localNetworkGuard.package.catalog.signer_subject = "CN=ChatOS Test Driver"
    $localNetworkGuard | ConvertTo-Json -Depth 12 | Set-Content $networkGuardPath -Encoding utf8
    foreach ($path in @($wdkX64Path, $wdkArm64Path)) {
        $localWdk = Get-Content $path -Raw | ConvertFrom-Json
        $localWdk.signing_mode = "local_test"
        $localWdk.driver_signer_subject = "CN=ChatOS Test Driver"
        $localWdk.catalog_signer_subject = "CN=ChatOS Test Driver"
        $localWdk | ConvertTo-Json -Depth 12 | Set-Content $path -Encoding utf8
    }
    $parameters.OutputPath = Join-Path $fixtureRoot "local-test-signature-rejected-summary.json"
    $localRejected = $false
    try {
        & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    }
    catch { $localRejected = $true }
    if (-not $localRejected) { throw "A locally test-signed driver unexpectedly passed formal verification." }
    $localFailure = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($localFailure.passed -ne $false -or
        @($localFailure.failures) -notcontains "networkguard.signing_mode" -or
        @($localFailure.failures) -notcontains "wdk_x64.signing_mode" -or
        @($localFailure.failures) -notcontains "wdk_arm64.signing_mode") {
        throw "Formal verification did not reject all locally test-signed driver evidence."
    }

    $parameters.AllowTestSignedDriver = $true
    $parameters.OutputPath = Join-Path $fixtureRoot "local-test-signature-allowed-summary.json"
    & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    $localAllowed = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($localAllowed.passed -ne $true -or @($localAllowed.failures).Count -ne 0) {
        throw "Explicit test-signing verification did not accept the local test package."
    }
    $parameters.Remove("AllowTestSignedDriver")
    $productionNetworkGuardJson | Set-Content $networkGuardPath -Encoding utf8
    $productionWdkX64Json | Set-Content $wdkX64Path -Encoding utf8
    $productionWdkArm64Json | Set-Content $wdkArm64Path -Encoding utf8

    New-TrxFixture $e2eTrxPath @("ChatOS.Connector.Tests.NetworkGuardEndToEndAcceptanceTests.$($e2eExpectedNames[0])")
    $parameters.OutputPath = Join-Path $fixtureRoot "missing-e2e-test-summary.json"
    $missingE2eFailureRaised = $false
    try {
        & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    }
    catch {
        $missingE2eFailureRaised = $true
    }
    if (-not $missingE2eFailureRaised) { throw "The incomplete NetworkGuard TRX fixture unexpectedly passed verification." }
    $missingE2eFailure = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($missingE2eFailure.passed -ne $false -or
        @($missingE2eFailure.failures) -notcontains "networkguard.test_run.invalid_test_run") {
        throw "The incomplete NetworkGuard TRX fixture did not record the expected test-run failure."
    }
    New-TrxFixture $e2eTrxPath @($e2eExpectedNames | ForEach-Object { "ChatOS.Connector.Tests.NetworkGuardEndToEndAcceptanceTests.$_" })

    $unsafeWdkReport = Get-Content $wdkX64Path -Raw | ConvertFrom-Json
    $unsafeWdkReport.artifacts[0].path = "../escape.sys"
    $unsafeWdkReport | ConvertTo-Json -Depth 12 | Set-Content $wdkX64Path -Encoding utf8
    $parameters.OutputPath = Join-Path $fixtureRoot "unsafe-artifact-summary.json"
    $unsafeFailureRaised = $false
    try {
        & (Join-Path $repoRoot "build\verify-windows-acceptance.ps1") @parameters
    }
    catch {
        $unsafeFailureRaised = $true
    }
    if (-not $unsafeFailureRaised) { throw "The unsafe WDK artifact fixture unexpectedly passed verification." }
    $unsafeFailure = Get-Content $parameters.OutputPath -Raw | ConvertFrom-Json
    if ($unsafeFailure.passed -ne $false -or @($unsafeFailure.failures) -notcontains "wdk_x64.artifact_0.path") {
        throw "The unsafe WDK artifact fixture did not record the expected path failure."
    }

    Write-Host "Windows acceptance evidence verifier tests passed."
}
finally {
    Remove-Item $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}
