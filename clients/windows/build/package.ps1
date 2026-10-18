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
$rustTarget = if ($Platform -eq "ARM64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
$monorepoRoot = (Resolve-Path (Join-Path $repoRoot "..\..")).Path
$desktopProject = Join-Path $repoRoot "src\ChatOS.Desktop\ChatOS.Desktop.csproj"
$signingEnabled = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
$packagesRoot = Join-Path $repoRoot "src\ChatOS.Desktop\AppPackages\$Platform"
$previousAwsLcNoAsm = $env:AWS_LC_SYS_NO_ASM
$previousAwsLcPrebuiltNasm = $env:AWS_LC_SYS_PREBUILT_NASM
$previousCMake = $env:CMAKE

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

# Packaging always builds an optimized Host. aws-lc-sys rejects NO_ASM in
# Release, so use its checked-in x64 NASM objects when NASM is unavailable.
# Apply these only after certificate validation so early validation failures do
# not mutate the caller's environment.
$env:AWS_LC_SYS_NO_ASM = $null
if ($Platform -eq "x64" -and -not (Get-Command nasm -ErrorAction SilentlyContinue)) {
    $env:AWS_LC_SYS_PREBUILT_NASM = "1"
}
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vsWhere -PathType Leaf) {
        $visualStudioRoot = & $vsWhere -latest -products * -property installationPath
        if ($visualStudioRoot) {
            $visualStudioCMake = Join-Path $visualStudioRoot "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
            if (Test-Path $visualStudioCMake -PathType Leaf) {
                $env:CMAKE = $visualStudioCMake
            }
        }
    }
}

# Match build.ps1: invoke the SDK from the monorepo root and pass an absolute
# project path. This avoids making packaging depend on the caller's directory
# while CI still installs the pinned .NET 8 SDK before invoking the script.
Push-Location $monorepoRoot
try {
    & (Join-Path $repoRoot "build\ensure-package-assets.ps1")
    cargo build --manifest-path (Join-Path $monorepoRoot "Cargo.toml") `
        -p chatos_local_agent_host --release --target $rustTarget
    if ($LASTEXITCODE -ne 0) {
        throw "Local Agent Host build failed with exit code $LASTEXITCODE."
    }
    $localAgentHost = Join-Path $monorepoRoot "target-shared\$rustTarget\release\chatos_local_agent_host.exe"
    if (-not (Test-Path $localAgentHost -PathType Leaf)) {
        throw "Local Agent Host executable was not produced at $localAgentHost."
    }
    if (Test-Path $packagesRoot) {
        Remove-Item $packagesRoot -Recurse -Force
    }
    $null = New-Item -ItemType Directory -Path $packagesRoot -Force

    $publishArguments = @(
        "publish",
        $desktopProject,
        "-c", "Release",
        "-p:Platform=$Platform",
        "-p:RuntimeIdentifier=$runtimeIdentifier",
        "-p:WindowsPackageType=MSIX",
        "-p:GenerateAppxPackageOnBuild=true",
        "-p:AppxBundle=Never",
        "-p:AppxPackageDir=$packagesRoot\",
        "-p:LocalAgentHostExecutable=$localAgentHost",
        "-p:RequireLocalAgentHost=true",
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
    $env:AWS_LC_SYS_NO_ASM = $previousAwsLcNoAsm
    $env:AWS_LC_SYS_PREBUILT_NASM = $previousAwsLcPrebuiltNasm
    $env:CMAKE = $previousCMake
}
