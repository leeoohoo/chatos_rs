[CmdletBinding()]
param(
    [ValidateSet("x64", "ARM64")]
    [string]$Platform,

    [ValidatePattern("^https?://")]
    [string]$ApiBaseUrl = "https://gateway.jgoool.com/api/chatos",

    [ValidatePattern("^https?://")]
    [string]$LocalConnectorCloudBaseUrl = "https://local-connector.jgoool.com",

    [ValidatePattern("^[0-9]+\.[0-9]+\.[0-9]+(?:\.[0-9]+)?$")]
    [string]$Version = "3.0.0",

    [switch]$SkipTests,

    [switch]$SkipToolInstall
)

$ErrorActionPreference = "Stop"
$isWindowsPlatform = if ($PSVersionTable.PSEdition -eq "Desktop") {
    $env:OS -eq "Windows_NT"
}
else {
    $IsWindows
}
if (-not $isWindowsPlatform) {
    throw "ChatOS Windows packaging must run on Windows 10/11."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$osArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ([string]::IsNullOrWhiteSpace($Platform)) {
    $Platform = if ($osArchitecture -eq "Arm64") { "ARM64" } else { "x64" }
}

$dotnetCommand = Get-Command dotnet -ErrorAction SilentlyContinue
if (-not $dotnetCommand) {
    throw ".NET 8 SDK is required. Install it from https://dotnet.microsoft.com/download/dotnet/8.0"
}
$sdkVersions = @(dotnet --list-sdks)
if (-not ($sdkVersions | Where-Object { $_ -match '^8\.' })) {
    throw ".NET 8 SDK is required. Installed SDKs: $($sdkVersions -join ', ')"
}

$runtimeIdentifier = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
$normalizedApiBaseUrl = $ApiBaseUrl.TrimEnd('/')
$normalizedConnectorBaseUrl = $LocalConnectorCloudBaseUrl.TrimEnd('/')
$desktopProject = Join-Path $repoRoot "src\ChatOS.Desktop\ChatOS.Desktop.csproj"
$artifactsRoot = Join-Path $repoRoot "BundleArtifacts"
$payloadRoot = Join-Path $artifactsRoot "payload-$Platform"
$installerRoot = Join-Path $artifactsRoot "installer-$Platform"
$installerScript = Join-Path $repoRoot "installer\ChatOS.iss"

function Find-InnoSetupCompiler {
    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }

    $candidates = @(
        (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
        (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe"),
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) }
    return $candidates | Where-Object { Test-Path $_ -PathType Leaf } | Select-Object -First 1
}

$innoCompiler = Find-InnoSetupCompiler
if (-not $innoCompiler -and -not $SkipToolInstall) {
    $winget = Get-Command winget.exe -ErrorAction SilentlyContinue
    if (-not $winget) {
        throw "Inno Setup 6 is required to create the EXE installer, and winget is unavailable. Install Inno Setup 6 from https://jrsoftware.org/isdl.php"
    }
    Write-Host "Installing Inno Setup 6 for EXE packaging..."
    winget install --id JRSoftware.InnoSetup --exact --silent `
        --accept-package-agreements --accept-source-agreements
    if ($LASTEXITCODE -ne 0) {
        throw "Inno Setup installation failed with exit code $LASTEXITCODE."
    }
    $innoCompiler = Find-InnoSetupCompiler
}
if (-not $innoCompiler) {
    throw "Inno Setup 6 was not found. Install it or rerun without -SkipToolInstall."
}

Push-Location $repoRoot
try {
    if (-not $SkipTests) {
        Write-Host "Running ChatOS Windows tests..."
        & (Join-Path $repoRoot "build\test.ps1") -Configuration Release
        if ($LASTEXITCODE -ne 0) {
            throw "ChatOS Windows tests failed with exit code $LASTEXITCODE."
        }
    }

    & (Join-Path $repoRoot "build\ensure-package-assets.ps1")

    if (Test-Path $payloadRoot) {
        Remove-Item $payloadRoot -Recurse -Force
    }
    if (Test-Path $installerRoot) {
        Remove-Item $installerRoot -Recurse -Force
    }
    $null = New-Item -ItemType Directory -Path $payloadRoot -Force
    $null = New-Item -ItemType Directory -Path $installerRoot -Force

    Write-Host "Publishing ChatOS Windows Release/$Platform..."
    dotnet publish $desktopProject `
        -c Release `
        -p:Platform=$Platform `
        -p:RuntimeIdentifier=$runtimeIdentifier `
        -p:WindowsPackageType=None `
        -p:WindowsAppSDKSelfContained=true `
        --self-contained true `
        --output $payloadRoot `
        --nologo
    if ($LASTEXITCODE -ne 0) {
        throw "ChatOS Windows publish failed with exit code $LASTEXITCODE."
    }

    $executable = Join-Path $payloadRoot "ChatOS.Desktop.exe"
    if (-not (Test-Path $executable -PathType Leaf)) {
        throw "Installer payload does not contain ChatOS.Desktop.exe."
    }

    $launcher = @"
@echo off
setlocal
set "CHATOS_API_BASE_URL=$normalizedApiBaseUrl"
set "CHATOS_LOCAL_CONNECTOR_CLOUD_BASE_URL=$normalizedConnectorBaseUrl"
start "" "%~dp0ChatOS.Desktop.exe"
endlocal
"@
    [IO.File]::WriteAllText(
        (Join-Path $payloadRoot "Start-ChatOS.cmd"),
        $launcher,
        [Text.UTF8Encoding]::new($false)
    )

    $sourceRevision = $null
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $sourceRevision = (& git -C $repoRoot rev-parse HEAD 2>$null | Select-Object -First 1)
    }
    $metadata = [ordered]@{
        schema_version = 1
        packaged_at = [DateTimeOffset]::UtcNow.ToString("O")
        platform = $Platform
        runtime_identifier = $runtimeIdentifier
        self_contained = $true
        api_base_url = $normalizedApiBaseUrl
        local_connector_cloud_base_url = $normalizedConnectorBaseUrl
        source_revision = if ([string]::IsNullOrWhiteSpace($sourceRevision)) { $null } else { $sourceRevision.Trim() }
        executable_sha256 = (Get-FileHash $executable -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $metadata | ConvertTo-Json -Depth 5 | Set-Content `
        (Join-Path $payloadRoot "package-metadata.json") `
        -Encoding utf8

    $instructions = @"
ChatOS Windows installation files

This directory is the installer payload. Install ChatOS with the generated ChatOS-Setup-$Platform.exe.

After installation, start ChatOS from the Start menu or desktop shortcut.

Configured services:
API: $normalizedApiBaseUrl
Local Connector: $normalizedConnectorBaseUrl
"@
    [IO.File]::WriteAllText(
        (Join-Path $payloadRoot "README.txt"),
        $instructions,
        [Text.UTF8Encoding]::new($false)
    )

    Write-Host "Building ChatOS EXE installer..."
    & $innoCompiler `
        "/DSourceDir=$payloadRoot" `
        "/DOutputDir=$installerRoot" `
        "/DTargetPlatform=$Platform" `
        "/DAppVersion=$Version" `
        $installerScript
    if ($LASTEXITCODE -ne 0) {
        throw "Inno Setup packaging failed with exit code $LASTEXITCODE."
    }

    $installer = Join-Path $installerRoot "ChatOS-Setup-$Platform.exe"
    if (-not (Test-Path $installer -PathType Leaf)) {
        throw "EXE installer was not created: $installer"
    }
    Write-Host "EXE installer: $installer"
}
finally {
    Pop-Location
}
