[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet("x64", "ARM64")]
    [string]$Platform,

    [Parameter(Mandatory)]
    [ValidatePattern("^[0-9A-Fa-f]{40}$")]
    [string]$CertificateThumbprint,

    [string]$PackageDirectory,

    [string]$OutputDirectory = "artifacts\networkguard-submission",

    [ValidatePattern("^https://")]
    [string]$TimestampUrl = "https://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows) { throw "NetworkGuard Hardware Dev Center submission packages must be created on Windows." }
$repoRoot = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "networkguard-package-evidence.ps1")
$CertificateThumbprint = $CertificateThumbprint.ToUpperInvariant()
$certificate = Get-Item "Cert:\CurrentUser\My\$CertificateThumbprint" -ErrorAction SilentlyContinue
if (-not $certificate -or -not $certificate.HasPrivateKey) {
    throw "The submission-signing certificate is not installed with a private key."
}
$package = if ([string]::IsNullOrWhiteSpace($PackageDirectory)) {
    Join-Path $repoRoot "artifacts\networkguard\$Platform"
}
else {
    (Resolve-Path $PackageDirectory).Path
}
$driverRoot = Join-Path $package "driver"
Assert-NetworkGuardPackageManifest $package | Out-Null
foreach ($name in @(
    "ChatOS.NetworkGuard.Driver.sys",
    "ChatOS.NetworkGuard.Driver.cat",
    "ChatOS.NetworkGuard.Driver.inf")) {
    if (-not (Test-Path (Join-Path $driverRoot $name) -PathType Leaf)) {
        throw "The NetworkGuard base package is missing $name."
    }
}
$outputRoot = if ([IO.Path]::IsPathRooted($OutputDirectory)) {
    [IO.Path]::GetFullPath($OutputDirectory)
}
else {
    [IO.Path]::GetFullPath((Join-Path $repoRoot $OutputDirectory))
}
$normalizedOutputRoot = $outputRoot.TrimEnd('\', '/')
$volumeRoot = [IO.Path]::GetPathRoot($outputRoot).TrimEnd('\', '/')
if ($normalizedOutputRoot -eq $volumeRoot -or
    $normalizedOutputRoot -eq $repoRoot.TrimEnd('\', '/')) {
    throw "OutputDirectory must be a dedicated submission directory, not a volume or repository root."
}
$outputPrefix = $normalizedOutputRoot + [IO.Path]::DirectorySeparatorChar
$normalizedPackage = $package.TrimEnd('\', '/')
if ($normalizedPackage -eq $normalizedOutputRoot -or
    $normalizedPackage.StartsWith($outputPrefix, [StringComparison]::OrdinalIgnoreCase)) {
    throw "OutputDirectory must not contain or replace the source package directory."
}
$platformOutput = Join-Path $outputRoot $Platform
if (Test-Path $platformOutput) { Remove-Item $platformOutput -Recurse -Force }
$null = New-Item -ItemType Directory -Path $platformOutput -Force
$cabName = "ChatOS-NetworkGuard-$Platform-hardware-submission.cab"
$cabPath = Join-Path $platformOutput $cabName
$ddfPath = Join-Path $platformOutput "submission.ddf"
$ddf = @"
.OPTION EXPLICIT
.Set CabinetNameTemplate=$cabName
.Set DiskDirectoryTemplate=$platformOutput
.Set CompressionType=MSZIP
.Set Cabinet=on
.Set Compress=on
"$(Join-Path $driverRoot 'ChatOS.NetworkGuard.Driver.sys')" ChatOS.NetworkGuard.Driver.sys
"$(Join-Path $driverRoot 'ChatOS.NetworkGuard.Driver.cat')" ChatOS.NetworkGuard.Driver.cat
"$(Join-Path $driverRoot 'ChatOS.NetworkGuard.Driver.inf')" ChatOS.NetworkGuard.Driver.inf
"@
[IO.File]::WriteAllText($ddfPath, $ddf)
try {
    & makecab.exe /F $ddfPath | Out-Host
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $cabPath -PathType Leaf)) {
        throw "Hardware Dev Center submission CAB creation failed."
    }
}
finally {
    Remove-Item $ddfPath -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $platformOutput "setup.inf") -Force -ErrorAction SilentlyContinue
    Remove-Item (Join-Path $platformOutput "setup.rpt") -Force -ErrorAction SilentlyContinue
}

$signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Filter signtool.exe -File -Recurse |
    Where-Object { $_.FullName -like "*\x64\signtool.exe" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1
if (-not $signtool) { throw "signtool.exe was not found." }
& $signtool.FullName sign /sha1 $CertificateThumbprint /fd SHA256 /tr $TimestampUrl /td SHA256 $cabPath
if ($LASTEXITCODE -ne 0) { throw "Hardware Dev Center submission CAB signing failed." }
$signature = Get-AuthenticodeSignature $cabPath
if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or
    $signature.SignerCertificate.Thumbprint -ne $CertificateThumbprint -or
    -not $signature.TimeStamperCertificate) {
    throw "Hardware Dev Center submission CAB signature verification failed."
}
$report = [ordered]@{
    schema_version = 1
    passed = $true
    created_at = [DateTimeOffset]::UtcNow.ToString("O")
    platform = $Platform
    source_package = Split-Path $package -Leaf
    cab = [ordered]@{
        file_name = $cabName
        length = (Get-Item $cabPath).Length
        sha256 = (Get-FileHash $cabPath -Algorithm SHA256).Hash.ToLowerInvariant()
        signature_status = $signature.Status.ToString()
        signer_subject = $signature.SignerCertificate.Subject
        signer_thumbprint = $signature.SignerCertificate.Thumbprint
        timestamp_subject = $signature.TimeStamperCertificate.Subject
    }
}
$report | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $platformOutput "submission-report.json") -Encoding utf8
Write-Host "Hardware Dev Center submission CAB: $cabPath"
