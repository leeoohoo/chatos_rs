[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidateSet("Install", "Upgrade", "Uninstall", "Status")]
    [string]$Action,

    [string]$PackageDirectory,

    [string]$PolicyKeyId,

    [string]$PolicyPublicKey,

    [switch]$AllowUnsignedPackage,

    [switch]$AllowTestSignedPackage,

    [switch]$ConfirmNoControlledProcesses
)

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "networkguard-package-evidence.ps1")
if (-not $IsWindows) { throw "NetworkGuard lifecycle operations require Windows." }
$principal = [Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw "Run this script from an elevated PowerShell window."
}
if ($AllowUnsignedPackage -and $AllowTestSignedPackage) {
    throw "AllowUnsignedPackage and AllowTestSignedPackage are mutually exclusive."
}
$serviceName = "ChatOSNetworkGuardService"
$driverName = "ChatOSNetworkGuard"

function Invoke-Sc([string[]]$Arguments, [switch]$AllowFailure) {
    & sc.exe @Arguments | Out-Host
    if (-not $AllowFailure -and $LASTEXITCODE -ne 0) { throw "sc.exe $($Arguments -join ' ') failed." }
}

function Remove-NetworkGuardDriverPackages {
    $packages = @(Get-WindowsDriver -Online |
        Where-Object {
            $_.OriginalFileName -and
            [IO.Path]::GetFileName($_.OriginalFileName) -eq "ChatOS.NetworkGuard.Driver.inf"
        })
    foreach ($package in $packages) {
        & pnputil.exe /delete-driver $package.Driver /uninstall /force | Out-Host
        if ($LASTEXITCODE -ne 0) {
            throw "Could not remove existing NetworkGuard driver package $($package.Driver)."
        }
    }
}

if ($Action -eq "Status") {
    Invoke-Sc @("query", $driverName) -AllowFailure
    Invoke-Sc @("query", $serviceName) -AllowFailure
    exit 0
}
if ($Action -eq "Uninstall" -and -not $ConfirmNoControlledProcesses) {
    throw "Uninstall can remove fail-closed filters. Stop all Controlled tasks, then pass -ConfirmNoControlledProcesses."
}
if ($Action -in "Install", "Upgrade") {
    if (-not $PackageDirectory) { throw "PackageDirectory is required." }
    if ([string]::IsNullOrWhiteSpace($PolicyKeyId) -or $PolicyKeyId -notmatch '^[A-Za-z0-9._-]{1,128}$') {
        throw "PolicyKeyId is required and must contain only letters, numbers, dot, underscore or hyphen."
    }
    if ([string]::IsNullOrWhiteSpace($PolicyPublicKey) -or -not $PolicyPublicKey.StartsWith("ed25519:")) {
        throw "PolicyPublicKey is required in ed25519:<base64url> form."
    }
    $package = (Resolve-Path $PackageDirectory).Path
    $manifestPath = Join-Path $package "manifest.json"
    $buildReportPath = Join-Path $package "build-report.json"
    $inf = Join-Path $package "driver\ChatOS.NetworkGuard.Driver.inf"
    $exe = Join-Path $package "service\ChatOS.NetworkGuard.Service.exe"
    if (-not (Test-Path $manifestPath) -or
        -not (Test-Path $buildReportPath) -or
        -not (Test-Path $inf) -or
        -not (Test-Path $exe)) {
        throw "NetworkGuard package is incomplete."
    }
    $buildReport = Get-Content $buildReportPath -Raw | ConvertFrom-Json
    $signingMode = [string]$buildReport.signing_mode
    if ($signingMode -notin "unsigned", "local_test", "microsoft_production") {
        throw "NetworkGuard package signing provenance is missing or unsupported."
    }
    if ($signingMode -eq "unsigned" -and -not $AllowUnsignedPackage) {
        throw "Unsigned NetworkGuard packages require AllowUnsignedPackage on an isolated test-signing machine."
    }
    if ($signingMode -eq "local_test" -and -not $AllowTestSignedPackage) {
        throw "Locally test-signed NetworkGuard packages require AllowTestSignedPackage."
    }
    Assert-NetworkGuardPackageManifest $package | Out-Null
    $driverSignature = Get-AuthenticodeSignature (Join-Path $package "driver\ChatOS.NetworkGuard.Driver.sys")
    $catalogSignature = Get-AuthenticodeSignature (Join-Path $package "driver\ChatOS.NetworkGuard.Driver.cat")
    if ($driverSignature.Status -notin "Valid", "NotSigned") {
        throw "Driver signature is invalid: $($driverSignature.Status)"
    }
    if ($catalogSignature.Status -notin "Valid", "NotSigned") {
        throw "Driver catalog signature is invalid: $($catalogSignature.Status)"
    }
    if (($driverSignature.Status -eq "Valid") -xor ($catalogSignature.Status -eq "Valid")) {
        throw "Driver SYS and catalog must either both be signed or both be explicitly unsigned for test mode."
    }
    if ($signingMode -eq "unsigned" -and $driverSignature.Status -ne "NotSigned") {
        throw "The package claims unsigned mode but the driver artifacts are signed."
    }
    if ($signingMode -in "local_test", "microsoft_production" -and
        $driverSignature.Status -ne "Valid") {
        throw "The package signing mode requires trusted SYS and catalog signatures."
    }
    if ($signingMode -eq "microsoft_production") {
        if ($driverSignature.SignerCertificate.Subject -notmatch "Microsoft Windows Hardware Compatibility Publisher" -or
            $catalogSignature.SignerCertificate.Subject -notmatch "Microsoft Windows Hardware Compatibility Publisher") {
            throw "Production NetworkGuard packages must be signed by Microsoft Windows Hardware Compatibility Publisher."
        }
        if (-not $driverSignature.TimeStamperCertificate -or -not $catalogSignature.TimeStamperCertificate) {
            throw "Production NetworkGuard driver signatures must contain trusted timestamps."
        }
    }
    if ($Action -eq "Upgrade") {
        Invoke-Sc @("stop", $serviceName) -AllowFailure
        Invoke-Sc @("stop", $driverName) -AllowFailure
        Remove-NetworkGuardDriverPackages
    }
    & pnputil.exe /add-driver $inf /install | Out-Host
    if ($LASTEXITCODE -ne 0) { throw "Driver installation failed." }
    Invoke-Sc @("create", $serviceName, "binPath=", "`"$exe`"", "start=", "auto", "obj=", "LocalSystem") -AllowFailure
    Invoke-Sc @("config", $serviceName, "binPath=", "`"$exe`"", "start=", "auto", "obj=", "LocalSystem", "depend=", $driverName)
    $serviceRegistry = "HKLM:\SYSTEM\CurrentControlSet\Services\$serviceName"
    $environment = @(
        "ChatOS__NetworkGuard__TrustedPolicyPublicKeys__$PolicyKeyId=$PolicyPublicKey"
    )
    $null = New-ItemProperty $serviceRegistry -Name Environment -PropertyType MultiString -Value $environment -Force
    Invoke-Sc @("failure", $serviceName, "reset=", "86400", "actions=", "restart/5000/restart/15000/none/0")
    Invoke-Sc @("start", $driverName) -AllowFailure
    Invoke-Sc @("start", $serviceName)
    exit 0
}

Invoke-Sc @("stop", $serviceName) -AllowFailure
Invoke-Sc @("delete", $serviceName) -AllowFailure
Invoke-Sc @("stop", $driverName) -AllowFailure
Invoke-Sc @("delete", $driverName) -AllowFailure
Remove-NetworkGuardDriverPackages
Write-Host "The NetworkGuard service, driver service, and published driver packages were removed."
