[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet("x64", "ARM64")]
    [string]$RuntimePlatform,

    [Parameter(Mandatory)]
    [string]$MsixPackage,

    [string]$PreviousMsixPackage,

    [Parameter(Mandatory)]
    [string]$NetworkGuardPackageDirectory,

    [Parameter(Mandatory)]
    [string]$NativeReport,

    [Parameter(Mandatory)]
    [string]$MsixReport,

    [Parameter(Mandatory)]
    [string]$NetworkGuardReport,

    [Parameter(Mandatory)]
    [string]$WdkX64Report,

    [Parameter(Mandatory)]
    [string]$WdkArm64Report,

    [string]$OutputPath = "artifacts\windows-final-acceptance\verification-report.json",

    [switch]$RequireAuthenticatedUi,

    [switch]$RequireUpgrade,

    [switch]$RequireIpv6Literal,

    [switch]$AllowUnsignedMsix,

    [switch]$AllowUnsignedDriver,

    [switch]$AllowTestSignedDriver
)

$ErrorActionPreference = "Stop"
if ($AllowUnsignedDriver -and $AllowTestSignedDriver) {
    throw "AllowUnsignedDriver and AllowTestSignedDriver are mutually exclusive."
}
. (Join-Path $PSScriptRoot "trx-evidence.ps1")
$failures = [Collections.Generic.List[string]]::new()

function Add-Failure {
    param([Parameter(Mandatory)] [string]$Code)
    if (-not $failures.Contains($Code)) { $failures.Add($Code) }
}

function Get-PropertyValue {
    param(
        [AllowNull()] [object]$Object,
        [Parameter(Mandatory)] [string]$Name
    )
    if ($null -eq $Object) { return $null }
    $property = $Object.PSObject.Properties[$Name]
    if ($null -eq $property) { return $null }
    return $property.Value
}

function Read-Report {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string]$Label
    )
    if (-not (Test-Path $Path -PathType Leaf)) {
        Add-Failure "$Label.report_missing"
        return $null
    }
    try {
        return Get-Content $Path -Raw | ConvertFrom-Json
    }
    catch {
        Add-Failure "$Label.report_invalid_json"
        return $null
    }
}

function Test-ReportPassed {
    param(
        [AllowNull()] [object]$Report,
        [Parameter(Mandatory)] [string]$Label,
        [int]$MinimumSchemaVersion = 1
    )
    if ($null -eq $Report) { return }
    if ([int](Get-PropertyValue $Report "schema_version") -lt $MinimumSchemaVersion) {
        Add-Failure "$Label.schema_version"
    }
    if ((Get-PropertyValue $Report "passed") -ne $true) {
        Add-Failure "$Label.not_passed"
    }
}

function Test-CheckPassed {
    param(
        [AllowNull()] [object]$Report,
        [Parameter(Mandatory)] [string]$Label,
        [Parameter(Mandatory)] [string]$Name,
        [switch]$RequireRequested
    )
    $checks = Get-PropertyValue $Report "checks"
    $check = Get-PropertyValue $checks $Name
    if ($null -eq $check) {
        Add-Failure "$Label.check.$Name.missing"
        return
    }
    if ($RequireRequested -and (Get-PropertyValue $check "requested") -ne $true) {
        Add-Failure "$Label.check.$Name.not_requested"
    }
    if ((Get-PropertyValue $check "passed") -ne $true) {
        Add-Failure "$Label.check.$Name.failed"
    }
}

function Get-ReportEvidence {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [AllowNull()] [object]$Report
    )
    if (-not (Test-Path $Path -PathType Leaf)) { return $null }
    $file = Get-Item $Path
    return [ordered]@{
        file_name = $file.Name
        length = $file.Length
        sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        schema_version = Get-PropertyValue $Report "schema_version"
        passed = Get-PropertyValue $Report "passed"
    }
}

function Get-FileReference {
    param([Parameter(Mandatory)] [string]$Path)
    if (-not (Test-Path $Path -PathType Leaf)) { return $null }
    $file = Get-Item $Path
    return [ordered]@{
        file_name = $file.Name
        length = $file.Length
        sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

function Test-FileEvidence {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [AllowNull()] [object]$Evidence,
        [Parameter(Mandatory)] [string]$Label
    )
    if (-not (Test-Path $Path -PathType Leaf)) {
        Add-Failure "$Label.file_missing"
        return
    }
    if ($null -eq $Evidence) {
        Add-Failure "$Label.evidence_missing"
        return
    }
    $file = Get-Item $Path
    if ([long](Get-PropertyValue $Evidence "length") -ne $file.Length) {
        Add-Failure "$Label.length"
    }
    $expectedHash = [string](Get-PropertyValue $Evidence "sha256")
    $actualHash = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([string]::IsNullOrWhiteSpace($expectedHash) -or $expectedHash -ine $actualHash) {
        Add-Failure "$Label.sha256"
    }
}

function Test-TrxReportEvidence {
    param(
        [Parameter(Mandatory)] [string]$ReportPath,
        [AllowNull()] [object]$Evidence,
        [Parameter(Mandatory)] [string]$Label,
        [string[]]$ExpectedTestNames = @(),
        [int]$MinimumTestCount = 1
    )
    if ($null -eq $Evidence) {
        Add-Failure "$Label.evidence_missing"
        return
    }
    $relativePath = [string](Get-PropertyValue $Evidence "path")
    if ([string]::IsNullOrWhiteSpace($relativePath) -or
        [IO.Path]::IsPathRooted($relativePath) -or
        $relativePath -match '^[A-Za-z]:' -or
        $relativePath.StartsWith('\\') -or
        $relativePath.Replace('\', '/').Split('/') -contains '..') {
        Add-Failure "$Label.path"
        return
    }
    $reportRoot = Split-Path ([IO.Path]::GetFullPath($ReportPath)) -Parent
    $reportRootPrefix = $reportRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $normalizedPath = $relativePath.Replace('\', [IO.Path]::DirectorySeparatorChar).Replace('/', [IO.Path]::DirectorySeparatorChar)
    $candidate = [IO.Path]::GetFullPath((Join-Path $reportRoot $normalizedPath))
    if (-not $candidate.StartsWith($reportRootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        Add-Failure "$Label.path_escape"
        return
    }
    Test-FileEvidence $candidate $Evidence $Label
    if (-not (Test-Path $candidate -PathType Leaf)) { return }
    try {
        $actual = Assert-TrxTestRun `
            -Path $candidate `
            -EvidenceRoot $reportRoot `
            -ExpectedTestNames $ExpectedTestNames `
            -MinimumTestCount $MinimumTestCount
    }
    catch {
        Add-Failure "$Label.invalid_test_run"
        return
    }
    foreach ($property in @(
        "expected_test_count",
        "minimum_test_count",
        "executed_test_count",
        "passed_test_count",
        "failed_test_count")) {
        if ((Get-PropertyValue $Evidence $property) -ne $actual[$property]) {
            Add-Failure "$Label.$property"
        }
    }
}

function Test-WdkReport {
    param(
        [AllowNull()] [object]$Report,
        [Parameter(Mandatory)] [string]$Label,
        [Parameter(Mandatory)] [string]$ExpectedPlatform,
        [Parameter(Mandatory)] [string]$ReportPath,
        [Parameter(Mandatory)] [string[]]$AllowedSigningModes
    )
    Test-ReportPassed $Report $Label 2
    if ($null -eq $Report) { return }
    if ((Get-PropertyValue $Report "platform") -ne $ExpectedPlatform) {
        Add-Failure "$Label.platform"
    }
    if ((Get-PropertyValue $Report "configuration") -ne "Release") {
        Add-Failure "$Label.configuration"
    }
    if ([string]::IsNullOrWhiteSpace([string](Get-PropertyValue $Report "wdk_version"))) {
        Add-Failure "$Label.wdk_version"
    }
    $signingMode = [string](Get-PropertyValue $Report "signing_mode")
    if ($signingMode -notin $AllowedSigningModes) {
        Add-Failure "$Label.signing_mode"
    }
    if ($signingMode -in "local_test", "microsoft_production") {
        if ((Get-PropertyValue $Report "signed") -ne $true) { Add-Failure "$Label.not_signed" }
        if ((Get-PropertyValue $Report "driver_signature_status") -ne "Valid") {
            Add-Failure "$Label.driver_signature"
        }
        if ((Get-PropertyValue $Report "catalog_signature_status") -ne "Valid") {
            Add-Failure "$Label.catalog_signature"
        }
    }
    if ($signingMode -eq "microsoft_production") {
        if ([string](Get-PropertyValue $Report "driver_signer_subject") -notmatch
            "Microsoft Windows Hardware Compatibility Publisher") {
            Add-Failure "$Label.driver_production_signer"
        }
        if ([string](Get-PropertyValue $Report "catalog_signer_subject") -notmatch
            "Microsoft Windows Hardware Compatibility Publisher") {
            Add-Failure "$Label.catalog_production_signer"
        }
    }
    $artifactPaths = @((Get-PropertyValue $Report "artifacts") | ForEach-Object { [string]$_.path })
    foreach ($requiredSuffix in @(
        "driver/ChatOS.NetworkGuard.Driver.sys",
        "driver/ChatOS.NetworkGuard.Driver.cat",
        "driver/ChatOS.NetworkGuard.Driver.inf",
        "service/ChatOS.NetworkGuard.Service.exe")) {
        if (-not ($artifactPaths | Where-Object { $_.Replace('\', '/').EndsWith($requiredSuffix, [StringComparison]::OrdinalIgnoreCase) })) {
            Add-Failure "$Label.artifact.$($requiredSuffix.Replace('/', '_'))"
        }
    }
    $reportRoot = Split-Path ([IO.Path]::GetFullPath($ReportPath)) -Parent
    $reportRootPrefix = $reportRoot.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $artifactIndex = 0
    foreach ($artifact in @((Get-PropertyValue $Report "artifacts"))) {
        $artifactPath = [string](Get-PropertyValue $artifact "path")
        $artifactLabel = "$Label.artifact_$artifactIndex"
        $artifactIndex++
        if ([string]::IsNullOrWhiteSpace($artifactPath) -or
            [IO.Path]::IsPathRooted($artifactPath) -or
            $artifactPath -match '^[A-Za-z]:' -or
            $artifactPath.StartsWith('\\') -or
            $artifactPath.Replace('\', '/').Split('/') -contains '..') {
            Add-Failure "$artifactLabel.path"
            continue
        }
        $normalizedPath = $artifactPath.Replace('\', [IO.Path]::DirectorySeparatorChar).Replace('/', [IO.Path]::DirectorySeparatorChar)
        $candidate = [IO.Path]::GetFullPath((Join-Path $reportRoot $normalizedPath))
        if (-not $candidate.StartsWith($reportRootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
            Add-Failure "$artifactLabel.path_escape"
            continue
        }
        Test-FileEvidence $candidate $artifact $artifactLabel
    }
}

$native = Read-Report $NativeReport "native"
$msix = Read-Report $MsixReport "msix"
$networkGuard = Read-Report $NetworkGuardReport "networkguard"
$wdkX64 = Read-Report $WdkX64Report "wdk_x64"
$wdkArm64 = Read-Report $WdkArm64Report "wdk_arm64"

Test-ReportPassed $native "native" 3
if ($native) {
    if ((Get-PropertyValue $native "platform") -ne $RuntimePlatform) { Add-Failure "native.platform" }
    foreach ($name in @("windows_native_tests", "networkguard_service_tests", "desktop_build", "ui_smoke")) {
        Test-CheckPassed $native "native" $name -RequireRequested
    }
    $nativeUi = Get-PropertyValue (Get-PropertyValue $native "checks") "ui_smoke"
    if ($RequireAuthenticatedUi -and (Get-PropertyValue $nativeUi "authenticated") -ne $true) {
        Add-Failure "native.ui_not_authenticated"
    }
    $nativeTestRuns = Get-PropertyValue $native "test_runs"
    Test-TrxReportEvidence `
        -ReportPath $NativeReport `
        -Evidence (Get-PropertyValue $nativeTestRuns "windows_native") `
        -Label "native.test_run.windows_native" `
        -ExpectedTestNames @(
            "NativeCommandExecutorCapturesOutputOnWindows",
            "NativeCommandTimeoutReclaimsChildProcessTreeOnWindows",
            "AppContainerEnforcesWorkspaceAclAndNetworkCapabilitiesOnWindows",
            "ConPtyAcceptsInputAndProducesOutputOnWindows",
            "ControlledCommandAcquiresLeaseBeforeSuspendedProcessCanRunOnWindows",
            "ControlledConPtyAcquiresLeaseBeforeSuspendedShellCanRunOnWindows",
            "ControlledConPtyAcquireFailureNeverResumesProcessOnWindows",
            "ControlledAppContainerProfileAndWorkspaceAclAreRemovedAfterUseOnWindows",
            "CredentialManagerRoundTripsAndDeletesRandomSecretOnWindows") `
        -MinimumTestCount 9
    Test-TrxReportEvidence `
        -ReportPath $NativeReport `
        -Evidence (Get-PropertyValue $nativeTestRuns "networkguard_service") `
        -Label "native.test_run.networkguard_service" `
        -MinimumTestCount 19
}

Test-ReportPassed $msix "msix" 2
if ($msix) {
    Test-FileEvidence $MsixPackage (Get-PropertyValue $msix "package") "msix.package"
    $identity = Get-PropertyValue $msix "identity"
    $expectedArchitecture = if ($RuntimePlatform -eq "ARM64") { "arm64" } else { "x64" }
    if ([string](Get-PropertyValue $identity "architecture") -ine $expectedArchitecture) {
        Add-Failure "msix.architecture"
    }
    if (-not $AllowUnsignedMsix -and (Get-PropertyValue $msix "signature_status") -ne "Valid") {
        Add-Failure "msix.signature"
    }
    if ((Get-PropertyValue $msix "kept_installed") -eq $true -or
        [int](Get-PropertyValue $msix "remaining_package_count") -ne 0) {
        Add-Failure "msix.cleanup_residue"
    }
    foreach ($name in @("signature_policy", "current_package_install", "ui_smoke", "cleanup")) {
        Test-CheckPassed $msix "msix" $name -RequireRequested
    }
    $previousInstall = Get-PropertyValue (Get-PropertyValue $msix "checks") "previous_package_install"
    if ($RequireUpgrade) {
        if ([string]::IsNullOrWhiteSpace($PreviousMsixPackage)) {
            Add-Failure "msix.previous_package_file_missing"
        }
        else {
            Test-FileEvidence $PreviousMsixPackage (Get-PropertyValue $msix "previous_package") "msix.previous_package"
        }
        if ((Get-PropertyValue $previousInstall "requested") -ne $true -or
            (Get-PropertyValue $previousInstall "passed") -ne $true) {
            Add-Failure "msix.upgrade_not_proven"
        }
        if (-not $AllowUnsignedMsix -and (Get-PropertyValue $msix "previous_signature_status") -ne "Valid") {
            Add-Failure "msix.previous_signature"
        }
    }
    elseif (-not [string]::IsNullOrWhiteSpace($PreviousMsixPackage)) {
        Test-FileEvidence $PreviousMsixPackage (Get-PropertyValue $msix "previous_package") "msix.previous_package"
    }
    $msixUi = Get-PropertyValue (Get-PropertyValue $msix "checks") "ui_smoke"
    if ($RequireAuthenticatedUi -and (Get-PropertyValue $msixUi "authenticated") -ne $true) {
        Add-Failure "msix.ui_not_authenticated"
    }
}

Test-ReportPassed $networkGuard "networkguard" 3
if ($networkGuard) {
    if ((Get-PropertyValue $networkGuard "platform") -ne $RuntimePlatform) { Add-Failure "networkguard.platform" }
    if ((Get-PropertyValue $networkGuard "disruptive_restart_tests") -ne $true) {
        Add-Failure "networkguard.disruptive_not_run"
    }
    if ($RequireIpv6Literal -and (Get-PropertyValue $networkGuard "ipv6_literal_tested") -ne $true) {
        Add-Failure "networkguard.ipv6_literal_not_tested"
    }
    Test-TrxReportEvidence `
        -ReportPath $NetworkGuardReport `
        -Evidence (Get-PropertyValue $networkGuard "test_run") `
        -Label "networkguard.test_run" `
        -ExpectedTestNames @(
            "SignedPolicyAllowsOnlyApprovedHttpTlsAndLeavesNoLeaseResidue",
            "ServiceAndDriverRestartRemainFailClosedAndReconcileResidue") `
        -MinimumTestCount 2
    $package = Get-PropertyValue $networkGuard "package"
    $driver = Get-PropertyValue $package "driver"
    $catalog = Get-PropertyValue $package "catalog"
    $networkGuardSigningMode = [string](Get-PropertyValue $package "signing_mode")
    $allowedDriverSigningModes = if ($AllowUnsignedDriver) {
        @("unsigned", "local_test", "microsoft_production")
    }
    elseif ($AllowTestSignedDriver) {
        @("local_test", "microsoft_production")
    }
    else {
        @("microsoft_production")
    }
    if ($networkGuardSigningMode -notin $allowedDriverSigningModes) {
        Add-Failure "networkguard.signing_mode"
    }
    Test-FileEvidence (Join-Path $NetworkGuardPackageDirectory "driver\ChatOS.NetworkGuard.Driver.sys") $driver "networkguard.package.driver"
    Test-FileEvidence (Join-Path $NetworkGuardPackageDirectory "driver\ChatOS.NetworkGuard.Driver.cat") $catalog "networkguard.package.catalog"
    Test-FileEvidence (Join-Path $NetworkGuardPackageDirectory "manifest.json") (Get-PropertyValue $package "manifest") "networkguard.package.manifest"
    if (-not $AllowUnsignedDriver) {
        if ((Get-PropertyValue $driver "signature_status") -ne "Valid") { Add-Failure "networkguard.driver_signature" }
        if ((Get-PropertyValue $catalog "signature_status") -ne "Valid") { Add-Failure "networkguard.catalog_signature" }
        if ([string]::IsNullOrWhiteSpace([string](Get-PropertyValue $driver "signer_thumbprint"))) {
            Add-Failure "networkguard.driver_signer"
        }
        if ([string]::IsNullOrWhiteSpace([string](Get-PropertyValue $catalog "signer_thumbprint"))) {
            Add-Failure "networkguard.catalog_signer"
        }
        if ($networkGuardSigningMode -eq "local_test" -and
            (Get-PropertyValue $driver "signer_thumbprint") -ne (Get-PropertyValue $catalog "signer_thumbprint")) {
            Add-Failure "networkguard.signer_mismatch"
        }
    }
    if ($networkGuardSigningMode -eq "microsoft_production") {
        if ([string](Get-PropertyValue $driver "signer_subject") -notmatch
            "Microsoft Windows Hardware Compatibility Publisher") {
            Add-Failure "networkguard.driver_production_signer"
        }
        if ([string](Get-PropertyValue $catalog "signer_subject") -notmatch
            "Microsoft Windows Hardware Compatibility Publisher") {
            Add-Failure "networkguard.catalog_production_signer"
        }
        if ([string]::IsNullOrWhiteSpace([string](Get-PropertyValue $driver "timestamp_subject")) -or
            [string]::IsNullOrWhiteSpace([string](Get-PropertyValue $catalog "timestamp_subject"))) {
            Add-Failure "networkguard.production_timestamp"
        }
    }
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
        Test-CheckPassed $networkGuard "networkguard" $name -RequireRequested
    }
}

$allowedWdkSigningModes = if ($AllowUnsignedDriver) {
    @("unsigned", "local_test", "microsoft_production")
}
elseif ($AllowTestSignedDriver) {
    @("local_test", "microsoft_production")
}
else {
    @("microsoft_production")
}
Test-WdkReport $wdkX64 "wdk_x64" "x64" $WdkX64Report $allowedWdkSigningModes
Test-WdkReport $wdkArm64 "wdk_arm64" "ARM64" $WdkArm64Report $allowedWdkSigningModes

$outputFullPath = [IO.Path]::GetFullPath($OutputPath)
$outputDirectory = Split-Path $outputFullPath -Parent
$null = New-Item -ItemType Directory -Path $outputDirectory -Force
$summary = [ordered]@{
    schema_version = 2
    passed = $failures.Count -eq 0
    verified_at = [DateTimeOffset]::UtcNow.ToString("O")
    runtime_platform = $RuntimePlatform
    requirements = [ordered]@{
        authenticated_ui = [bool]$RequireAuthenticatedUi
        upgrade = [bool]$RequireUpgrade
        ipv6_literal = [bool]$RequireIpv6Literal
        unsigned_msix_allowed = [bool]$AllowUnsignedMsix
        unsigned_driver_allowed = [bool]$AllowUnsignedDriver
        test_signed_driver_allowed = [bool]$AllowTestSignedDriver
    }
    failures = @($failures)
    evidence = [ordered]@{
        msix_package = Get-FileReference $MsixPackage
        previous_msix_package = if ([string]::IsNullOrWhiteSpace($PreviousMsixPackage)) { $null } else { Get-FileReference $PreviousMsixPackage }
        networkguard_driver = Get-FileReference (Join-Path $NetworkGuardPackageDirectory "driver\ChatOS.NetworkGuard.Driver.sys")
        networkguard_catalog = Get-FileReference (Join-Path $NetworkGuardPackageDirectory "driver\ChatOS.NetworkGuard.Driver.cat")
        native = Get-ReportEvidence $NativeReport $native
        msix = Get-ReportEvidence $MsixReport $msix
        networkguard = Get-ReportEvidence $NetworkGuardReport $networkGuard
        wdk_x64 = Get-ReportEvidence $WdkX64Report $wdkX64
        wdk_arm64 = Get-ReportEvidence $WdkArm64Report $wdkArm64
    }
}
$summary | ConvertTo-Json -Depth 8 | Set-Content $outputFullPath -Encoding utf8

if ($failures.Count -ne 0) {
    throw "Windows acceptance evidence verification failed. See $outputFullPath."
}

Write-Host "Windows acceptance evidence verified. Summary: $outputFullPath"
