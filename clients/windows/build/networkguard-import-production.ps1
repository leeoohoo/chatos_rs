[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet("x64", "ARM64")]
    [string]$Platform,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path $_ -PathType Container })]
    [string]$BasePackageDirectory,

    [Parameter(Mandatory)]
    [ValidateScript({ Test-Path $_ -PathType Container })]
    [string]$MicrosoftSignedDriverDirectory,

    [string]$OutputDirectory,

    [string]$ExpectedSignerPattern = "Microsoft Windows Hardware Compatibility Publisher"
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows) { throw "Microsoft-signed NetworkGuard packages must be imported on Windows." }
$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "networkguard-package-evidence.ps1")
$basePackage = (Resolve-Path $BasePackageDirectory).Path
$signedRoot = (Resolve-Path $MicrosoftSignedDriverDirectory).Path
$output = if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
    Join-Path $repoRoot "artifacts\networkguard-production\$Platform"
}
elseif ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
}
else {
    [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDirectory))
}
$normalizedOutput = $output.TrimEnd('\', '/')
$volumeRoot = [IO.Path]::GetPathRoot($output).TrimEnd('\', '/')
if ($normalizedOutput -eq $volumeRoot -or $normalizedOutput -eq $repoRoot.TrimEnd('\', '/')) {
    throw "OutputDirectory must be a dedicated package directory, not a volume or repository root."
}
$outputPrefix = $normalizedOutput + [IO.Path]::DirectorySeparatorChar
foreach ($inputRoot in @($basePackage, $signedRoot)) {
    $normalizedInput = $inputRoot.TrimEnd('\', '/')
    if ($normalizedInput -eq $normalizedOutput -or
        $normalizedInput.StartsWith($outputPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "OutputDirectory must not contain or replace an input package directory."
    }
}

function Get-SingleSignedFile([string]$Name) {
    $matches = @(Get-ChildItem $signedRoot -Filter $Name -File -Recurse)
    if ($matches.Count -ne 1) {
        throw "The Microsoft-signed result must contain exactly one $Name file."
    }
    return $matches[0]
}

function Get-FileEvidence([IO.FileInfo]$File, [string]$Root) {
    return [ordered]@{
        path = $File.FullName.Substring($Root.Length + 1).Replace('\', '/')
        length = $File.Length
        sha256 = (Get-FileHash $File.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

$baseReportPath = Join-Path $basePackage "build-report.json"
$baseManifestPath = Join-Path $basePackage "manifest.json"
$baseInfPath = Join-Path $basePackage "driver\ChatOS.NetworkGuard.Driver.inf"
$baseServicePath = Join-Path $basePackage "service\ChatOS.NetworkGuard.Service.exe"
foreach ($required in @($baseReportPath, $baseManifestPath, $baseInfPath, $baseServicePath)) {
    if (-not (Test-Path $required -PathType Leaf)) {
        throw "The unsigned base package is incomplete: $required"
    }
}
$baseReport = Get-Content $baseReportPath -Raw | ConvertFrom-Json
Assert-NetworkGuardPackageManifest $basePackage | Out-Null
if ($baseReport.passed -ne $true -or $baseReport.platform -ne $Platform) {
    throw "The unsigned base package build report does not match platform $Platform."
}
if ($baseReport.signing_mode -notin $null, "unsigned", "local_test") {
    throw "The base package has an unsupported signing mode."
}

$signedDriver = Get-SingleSignedFile "ChatOS.NetworkGuard.Driver.sys"
$signedCatalog = Get-SingleSignedFile "ChatOS.NetworkGuard.Driver.cat"
$signedInf = Get-SingleSignedFile "ChatOS.NetworkGuard.Driver.inf"
if ((Get-FileHash $signedInf.FullName -Algorithm SHA256).Hash -ne
    (Get-FileHash $baseInfPath -Algorithm SHA256).Hash) {
    throw "The Microsoft-signed result contains an INF that differs from the submitted base package."
}

$driverSignature = Get-AuthenticodeSignature $signedDriver.FullName
$catalogSignature = Get-AuthenticodeSignature $signedCatalog.FullName
foreach ($entry in @(
    [ordered]@{ label = "driver"; signature = $driverSignature },
    [ordered]@{ label = "catalog"; signature = $catalogSignature })) {
    if ($entry.signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "The Microsoft-signed $($entry.label) signature is not trusted: $($entry.signature.Status)"
    }
    $subject = $entry.signature.SignerCertificate.Subject
    if ([string]::IsNullOrWhiteSpace($subject) -or $subject -notmatch $ExpectedSignerPattern) {
        throw "The $($entry.label) signer is not the expected Microsoft hardware publisher."
    }
    if (-not $entry.signature.TimeStamperCertificate) {
        throw "The Microsoft-signed $($entry.label) has no trusted timestamp."
    }
}

if (Test-Path $output) { Remove-Item $output -Recurse -Force }
$driverOutput = Join-Path $output "driver"
$serviceOutput = Join-Path $output "service"
$null = New-Item -ItemType Directory -Path $driverOutput -Force
$null = New-Item -ItemType Directory -Path $serviceOutput -Force
Copy-Item $signedDriver.FullName (Join-Path $driverOutput "ChatOS.NetworkGuard.Driver.sys")
Copy-Item $signedCatalog.FullName (Join-Path $driverOutput "ChatOS.NetworkGuard.Driver.cat")
Copy-Item $signedInf.FullName (Join-Path $driverOutput "ChatOS.NetworkGuard.Driver.inf")
Copy-Item $baseServicePath (Join-Path $serviceOutput "ChatOS.NetworkGuard.Service.exe")

$manifest = @(Get-ChildItem $output -File -Recurse | ForEach-Object { Get-FileEvidence $_ $output })
$manifest | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $output "manifest.json") -Encoding utf8
$report = [ordered]@{
    schema_version = 2
    passed = $true
    imported_at = [DateTimeOffset]::UtcNow.ToString("O")
    source_revision = $baseReport.source_revision
    configuration = $baseReport.configuration
    platform = $Platform
    wdk_version = $baseReport.wdk_version
    visual_studio = $baseReport.visual_studio
    signed = $true
    signing_mode = "microsoft_production"
    driver_signature_status = $driverSignature.Status.ToString()
    catalog_signature_status = $catalogSignature.Status.ToString()
    driver_signer_subject = $driverSignature.SignerCertificate.Subject
    catalog_signer_subject = $catalogSignature.SignerCertificate.Subject
    source_build_report_sha256 = (Get-FileHash $baseReportPath -Algorithm SHA256).Hash.ToLowerInvariant()
    source_manifest_sha256 = (Get-FileHash $baseManifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    artifacts = $manifest
}
$report | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $output "build-report.json") -Encoding utf8
Write-Host "Microsoft production-signed NetworkGuard package: $output"
