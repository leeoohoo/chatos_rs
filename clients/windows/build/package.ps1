[CmdletBinding()]
param(
    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64",

    [ValidatePattern("^[0-9A-Fa-f]{40}$")]
    [string]$CertificateThumbprint,

    [ValidatePattern("^https://")]
    [string]$TimestampUrl = "https://timestamp.digicert.com"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not $IsWindows) {
    throw "MSIX packaging must run on Windows."
}

$runtimeIdentifier = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
$signingEnabled = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
$packagesRoot = Join-Path $repoRoot "src\ChatOS.Desktop\AppPackages\$Platform"

if ($signingEnabled) {
    $CertificateThumbprint = $CertificateThumbprint.Replace(" ", "").ToUpperInvariant()
    $certificate = Get-Item "Cert:\CurrentUser\My\$CertificateThumbprint" -ErrorAction SilentlyContinue
    if (-not $certificate -or -not $certificate.HasPrivateKey) {
        throw "The MSIX signing certificate is not installed with a private key in Cert:\CurrentUser\My."
    }

    [xml]$manifest = Get-Content (Join-Path $repoRoot "src\ChatOS.Desktop\Package.appxmanifest")
    $manifestPublisher = $manifest.Package.Identity.Publisher
    if ($certificate.Subject -ne $manifestPublisher) {
        throw "Signing certificate subject '$($certificate.Subject)' does not match package Publisher '$manifestPublisher'."
    }
}

Push-Location $repoRoot
try {
    & (Join-Path $repoRoot "build\ensure-package-assets.ps1")
    if (Test-Path $packagesRoot) {
        Remove-Item $packagesRoot -Recurse -Force
    }
    $null = New-Item -ItemType Directory -Path $packagesRoot -Force

    $publishArguments = @(
        "publish",
        ".\src\ChatOS.Desktop\ChatOS.Desktop.csproj",
        "-c", "Release",
        "-p:Platform=$Platform",
        "-p:RuntimeIdentifier=$runtimeIdentifier",
        "-p:WindowsPackageType=MSIX",
        "-p:GenerateAppxPackageOnBuild=true",
        "-p:AppxBundle=Never",
        "-p:AppxPackageDir=$packagesRoot\",
        "-p:AppxPackageSigningEnabled=$($signingEnabled.ToString().ToLowerInvariant())",
        "--nologo"
    )
    if ($signingEnabled) {
        $publishArguments += "-p:PackageCertificateThumbprint=$CertificateThumbprint"
        if (-not [string]::IsNullOrWhiteSpace($TimestampUrl)) {
            $publishArguments += "-p:AppxPackageSigningTimestampServerUrl=$TimestampUrl"
        }
    }

    dotnet @publishArguments
    if ($LASTEXITCODE -ne 0) {
        throw "ChatOS Windows packaging failed with exit code $LASTEXITCODE."
    }

    $packages = @(Get-ChildItem $packagesRoot -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.Extension -in ".msix", ".msixbundle", ".appx", ".appxbundle" })
    if ($packages.Count -eq 0) {
        throw "Packaging completed without an MSIX/AppX output. The package manifest and signing pipeline still require Windows completion."
    }

    if ($signingEnabled) {
        foreach ($package in $packages) {
            $signature = Get-AuthenticodeSignature $package.FullName
            if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
                throw "Package signature validation failed for $($package.Name): $($signature.StatusMessage)"
            }
            if ($signature.SignerCertificate.Thumbprint -ne $CertificateThumbprint) {
                throw "Package $($package.Name) was signed with an unexpected certificate."
            }
            if (-not $signature.TimeStamperCertificate) {
                throw "Package $($package.Name) does not contain a trusted timestamp."
            }
        }
    }

    $packages | ForEach-Object { Write-Host "Package: $($_.FullName)" }
}
finally {
    Pop-Location
}
