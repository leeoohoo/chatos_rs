[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",

    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64",

    [string]$PackageDirectory,

    [string]$CertificateThumbprint,

    [string]$AllowedUrl = "https://example.com/",

    [string]$DeniedUrl = "https://www.example.com/",

    [string]$Ipv6Literal,

    [string]$OutputDirectory = "artifacts\networkguard-acceptance",

    [switch]$Disruptive,

    [switch]$UninstallAfter
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows) { throw "NetworkGuard acceptance requires Windows." }
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Run NetworkGuard acceptance from an elevated PowerShell window."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "trx-evidence.ps1")
. (Join-Path $PSScriptRoot "networkguard-package-evidence.ps1")
$evidenceRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
}
else {
    [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDirectory))
}
$null = New-Item -ItemType Directory -Path $evidenceRoot -Force
$runId = "{0}-{1}" -f `
    [DateTimeOffset]::UtcNow.ToString("yyyyMMdd-HHmmss"), `
    [Guid]::NewGuid().ToString("N").Substring(0, 8)
$runRoot = Join-Path $evidenceRoot "$Platform-$runId"
$keyRoot = Join-Path $runRoot "keys"
$resultsRoot = Join-Path $runRoot "results"
$null = New-Item -ItemType Directory -Path $keyRoot -Force
$null = New-Item -ItemType Directory -Path $resultsRoot -Force
$reportPath = Join-Path $runRoot "acceptance-report.json"
$startedAt = [DateTimeOffset]::UtcNow
$passed = $false
$failureType = $null
$failureStage = $null
$installed = $false
$packageBuilt = $false
$packageValidated = $false
$installedPassed = $false
$e2eTestsPassed = $false
$serviceStatusCaptured = $false
$uninstallPassed = $false
$currentStage = "policy_key_generation"
$driverSignatureStatus = $null
$catalogSignatureStatus = $null
$packageEvidence = $null
$e2eTestEvidence = $null
$packageSigningMode = $null
$allowedHostForReport = try { ([Uri]$AllowedUrl).IdnHost } catch { $null }
$deniedHostForReport = try { ([Uri]$DeniedUrl).IdnHost } catch { $null }

function Invoke-Checked {
    param(
        [Parameter(Mandatory)] [scriptblock]$Command,
        [Parameter(Mandatory)] [string]$FailureMessage
    )
    & $Command
    if ($LASTEXITCODE -ne 0) { throw $FailureMessage }
}

function Get-EvidenceFile([IO.FileInfo]$File) {
    return [ordered]@{
        path = $File.FullName.Substring($runRoot.Length + 1).Replace('\', '/')
        length = $File.Length
        sha256 = (Get-FileHash $File.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

Push-Location $repoRoot
try {
    $keyId = "networkguard-acceptance-$runId"
    $keyJson = & dotnet run `
        --project .\tools\ChatOS.NetworkGuard.PolicyKeyTool\ChatOS.NetworkGuard.PolicyKeyTool.csproj `
        -c $Configuration -- generate $keyRoot $keyId
    if ($LASTEXITCODE -ne 0) { throw "Controlled-network acceptance key generation failed." }
    $key = $keyJson | Select-Object -Last 1 | ConvertFrom-Json
    $currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    & icacls.exe $key.private_key_path /inheritance:r /grant:r "*$currentSid`:(R)" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "Could not restrict the acceptance private-key ACL." }

    if ([string]::IsNullOrWhiteSpace($PackageDirectory)) {
        $currentStage = "networkguard_package_build"
        & .\build\networkguard.ps1 `
            -Configuration $Configuration `
            -Platform $Platform `
            -CertificateThumbprint $CertificateThumbprint
        $PackageDirectory = Join-Path $repoRoot "artifacts\networkguard\$Platform"
        $packageBuilt = $true
    }
    $currentStage = "networkguard_package_validation"
    $PackageDirectory = (Resolve-Path $PackageDirectory).Path
    Assert-NetworkGuardPackageManifest $PackageDirectory | Out-Null
    $driverPath = Join-Path $PackageDirectory "driver\ChatOS.NetworkGuard.Driver.sys"
    $catalogPath = Join-Path $PackageDirectory "driver\ChatOS.NetworkGuard.Driver.cat"
    $manifestPath = Join-Path $PackageDirectory "manifest.json"
    $buildReportPath = Join-Path $PackageDirectory "build-report.json"
    if (-not (Test-Path $buildReportPath -PathType Leaf)) {
        throw "The NetworkGuard package has no build-report.json signing provenance."
    }
    $packageBuildReport = Get-Content $buildReportPath -Raw | ConvertFrom-Json
    $packageSigningMode = [string]$packageBuildReport.signing_mode
    if ($packageSigningMode -notin "unsigned", "local_test", "microsoft_production") {
        throw "The NetworkGuard package has an unsupported or missing signing mode."
    }
    $driverSignature = Get-AuthenticodeSignature $driverPath
    $catalogSignature = Get-AuthenticodeSignature $catalogPath
    $driverSignatureStatus = $driverSignature.Status.ToString()
    $catalogSignatureStatus = $catalogSignature.Status.ToString()
    if ($driverSignature.Status -eq "NotSigned") {
        $bootConfiguration = (& bcdedit.exe /enum) -join "`n"
        if ($bootConfiguration -notmatch '(?im)^testsigning\s+Yes\s*$') {
            throw "The NetworkGuard driver is unsigned and Windows test-signing mode is disabled. Supply CertificateThumbprint or a signed package."
        }
    }
    elseif ($driverSignature.Status -ne "Valid") {
        throw "The NetworkGuard driver signature is invalid: $($driverSignature.Status)"
    }
    if ($catalogSignature.Status -ne $driverSignature.Status) {
        throw "The NetworkGuard SYS and catalog signature states do not match."
    }
    if ($packageSigningMode -eq "unsigned" -and $driverSignature.Status -ne "NotSigned") {
        throw "The package claims unsigned mode but contains signed driver artifacts."
    }
    if ($packageSigningMode -in "local_test", "microsoft_production" -and
        $driverSignature.Status -ne "Valid") {
        throw "The package signing mode requires trusted SYS and catalog signatures."
    }
    if ($packageSigningMode -eq "microsoft_production") {
        foreach ($signature in @($driverSignature, $catalogSignature)) {
            if ($signature.SignerCertificate.Subject -notmatch "Microsoft Windows Hardware Compatibility Publisher") {
                throw "The production package is not signed by Microsoft Windows Hardware Compatibility Publisher."
            }
            if (-not $signature.TimeStamperCertificate) {
                throw "The production driver signature has no trusted timestamp."
            }
        }
    }
    $packageEvidence = [ordered]@{
        directory_name = Split-Path $PackageDirectory -Leaf
        signing_mode = $packageSigningMode
        driver = [ordered]@{
            length = (Get-Item $driverPath).Length
            sha256 = (Get-FileHash $driverPath -Algorithm SHA256).Hash.ToLowerInvariant()
            signature_status = $driverSignatureStatus
            signer_subject = if ($driverSignature.SignerCertificate) { $driverSignature.SignerCertificate.Subject } else { $null }
            signer_thumbprint = if ($driverSignature.SignerCertificate) { $driverSignature.SignerCertificate.Thumbprint } else { $null }
            signer_not_after = if ($driverSignature.SignerCertificate) { $driverSignature.SignerCertificate.NotAfter.ToUniversalTime().ToString("O") } else { $null }
            timestamp_subject = if ($driverSignature.TimeStamperCertificate) { $driverSignature.TimeStamperCertificate.Subject } else { $null }
        }
        catalog = [ordered]@{
            length = (Get-Item $catalogPath).Length
            sha256 = (Get-FileHash $catalogPath -Algorithm SHA256).Hash.ToLowerInvariant()
            signature_status = $catalogSignatureStatus
            signer_subject = if ($catalogSignature.SignerCertificate) { $catalogSignature.SignerCertificate.Subject } else { $null }
            signer_thumbprint = if ($catalogSignature.SignerCertificate) { $catalogSignature.SignerCertificate.Thumbprint } else { $null }
            signer_not_after = if ($catalogSignature.SignerCertificate) { $catalogSignature.SignerCertificate.NotAfter.ToUniversalTime().ToString("O") } else { $null }
            timestamp_subject = if ($catalogSignature.TimeStamperCertificate) { $catalogSignature.TimeStamperCertificate.Subject } else { $null }
        }
        manifest = [ordered]@{
            length = (Get-Item $manifestPath).Length
            sha256 = (Get-FileHash $manifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $packageValidated = $true

    $currentStage = "networkguard_install"
    $lifecycleParameters = @{
        Action = "Upgrade"
        PackageDirectory = $PackageDirectory
        PolicyKeyId = $key.key_id
        PolicyPublicKey = $key.public_key
    }
    if ($packageSigningMode -eq "unsigned") { $lifecycleParameters.AllowUnsignedPackage = $true }
    if ($packageSigningMode -eq "local_test") { $lifecycleParameters.AllowTestSignedPackage = $true }
    & .\build\networkguard-lifecycle.ps1 @lifecycleParameters
    $installed = $true
    $installedPassed = $true

    $env:CHATOS_NETWORKGUARD_END_TO_END = "1"
    $env:CHATOS_NETWORKGUARD_ACCEPTANCE_PRIVATE_KEY_PATH = $key.private_key_path
    $env:CHATOS_NETWORKGUARD_ACCEPTANCE_KEY_ID = $key.key_id
    $env:CHATOS_NETWORKGUARD_ACCEPTANCE_ALLOWED_URL = $AllowedUrl
    $env:CHATOS_NETWORKGUARD_ACCEPTANCE_DENIED_URL = $DeniedUrl
    $env:CHATOS_NETWORKGUARD_ACCEPTANCE_DISRUPTIVE = if ($Disruptive) { "1" } else { "0" }
    if ([string]::IsNullOrWhiteSpace($Ipv6Literal)) {
        Remove-Item Env:CHATOS_NETWORKGUARD_ACCEPTANCE_IPV6_LITERAL -ErrorAction SilentlyContinue
    }
    else {
        $env:CHATOS_NETWORKGUARD_ACCEPTANCE_IPV6_LITERAL = $Ipv6Literal
    }

    $currentStage = "networkguard_end_to_end_tests"
    Invoke-Checked {
        dotnet test .\tests\ChatOS.Connector.Tests\ChatOS.Connector.Tests.csproj `
            -c $Configuration `
            --filter "Category=NetworkGuardEndToEnd" `
            --logger "trx;LogFileName=networkguard-e2e-$Platform.trx" `
            --results-directory $resultsRoot `
            --nologo
    } "NetworkGuard end-to-end tests failed."
    $e2eTrxPath = Join-Path $resultsRoot "networkguard-e2e-$Platform.trx"
    $e2eTestEvidence = Assert-TrxTestRun `
        -Path $e2eTrxPath `
        -EvidenceRoot $runRoot `
        -ExpectedTestNames @(
            "SignedPolicyAllowsOnlyApprovedHttpTlsAndLeavesNoLeaseResidue",
            "ServiceAndDriverRestartRemainFailClosedAndReconcileResidue") `
        -MinimumTestCount 2
    $e2eTestsPassed = $true

    $currentStage = "networkguard_status_capture"
    & sc.exe query ChatOSNetworkGuard | Set-Content (Join-Path $resultsRoot "driver-status.txt") -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw "Could not capture NetworkGuard driver status." }
    & sc.exe query ChatOSNetworkGuardService | Set-Content (Join-Path $resultsRoot "service-status.txt") -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw "Could not capture NetworkGuard service status." }
    $serviceStatusCaptured = $true
    $passed = $true
}
catch {
    $failureType = $_.Exception.GetType().FullName
    $failureStage = $currentStage
    throw
}
finally {
    if ($UninstallAfter -and $installed) {
        $currentStage = "networkguard_uninstall"
        try {
            & .\build\networkguard-lifecycle.ps1 `
                -Action Uninstall `
                -ConfirmNoControlledProcesses
            $uninstallPassed = $true
        }
        catch {
            if ($passed) {
                $passed = $false
                $failureType = $_.Exception.GetType().FullName
                $failureStage = $currentStage
            }
        }
    }

    foreach ($name in @(
        "CHATOS_NETWORKGUARD_END_TO_END",
        "CHATOS_NETWORKGUARD_ACCEPTANCE_PRIVATE_KEY_PATH",
        "CHATOS_NETWORKGUARD_ACCEPTANCE_KEY_ID",
        "CHATOS_NETWORKGUARD_ACCEPTANCE_ALLOWED_URL",
        "CHATOS_NETWORKGUARD_ACCEPTANCE_DENIED_URL",
        "CHATOS_NETWORKGUARD_ACCEPTANCE_DISRUPTIVE",
        "CHATOS_NETWORKGUARD_ACCEPTANCE_IPV6_LITERAL")) {
        Remove-Item "Env:$name" -ErrorAction SilentlyContinue
    }

    if ($null -ne $key -and -not [string]::IsNullOrWhiteSpace($key.private_key_path)) {
        Remove-Item $key.private_key_path -Force -ErrorAction SilentlyContinue
    }
    Remove-Item $keyRoot -Recurse -Force -ErrorAction SilentlyContinue

    $files = @(Get-ChildItem $runRoot -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -ne $reportPath } |
        ForEach-Object { Get-EvidenceFile $_ })
    $report = [ordered]@{
        schema_version = 3
        passed = $passed
        failure_type = $failureType
        failure_stage = $failureStage
        started_at = $startedAt.ToString("O")
        finished_at = [DateTimeOffset]::UtcNow.ToString("O")
        configuration = $Configuration
        platform = $Platform
        allowed_host = $allowedHostForReport
        denied_host = $deniedHostForReport
        ipv6_literal_tested = -not [string]::IsNullOrWhiteSpace($Ipv6Literal)
        disruptive_restart_tests = [bool]$Disruptive
        package_built_during_acceptance = $packageBuilt
        package = $packageEvidence
        test_run = $e2eTestEvidence
        checks = [ordered]@{
            package_validation = [ordered]@{ requested = $true; passed = $packageValidated }
            install = [ordered]@{ requested = $true; passed = $installedPassed }
            signed_policy = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            allowed_http_tls = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            same_ip_denied_sni = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            ip_literals_fail_closed = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            dns_doh_quic_udp_fail_closed = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            no_sni_fail_closed = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            child_process_fail_closed = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            service_driver_restart_fail_closed = [ordered]@{
                requested = [bool]$Disruptive
                passed = $e2eTestsPassed -and [bool]$Disruptive
            }
            lease_residue_zero = [ordered]@{ requested = $true; passed = $e2eTestsPassed }
            status_capture = [ordered]@{ requested = $true; passed = $serviceStatusCaptured }
            uninstall = [ordered]@{ requested = [bool]$UninstallAfter; passed = $uninstallPassed }
        }
        artifacts = $files
    }
    $report | ConvertTo-Json -Depth 8 | Set-Content $reportPath -Encoding utf8
    Pop-Location
}

if (-not $passed) {
    throw "NetworkGuard acceptance failed. See $reportPath."
}
Write-Host "NetworkGuard acceptance passed. Evidence: $reportPath"
