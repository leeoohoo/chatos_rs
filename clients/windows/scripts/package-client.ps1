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
    throw "ChatOS Windows packaging must run on Windows 10/11 or Windows Server 2022."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$osArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ([string]::IsNullOrWhiteSpace($Platform)) {
    $Platform = if ($osArchitecture -eq "Arm64") { "ARM64" } else { "x64" }
}

function Save-RemoteFile {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Uri,

        [Parameter(Mandatory = $true)]
        [string]$Destination
    )

    if ([Net.ServicePointManager]::SecurityProtocol -band [Net.SecurityProtocolType]::Tls12) {
        # TLS 1.2 is already enabled.
    }
    else {
        [Net.ServicePointManager]::SecurityProtocol = `
            [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
    }
    Invoke-WebRequest -UseBasicParsing -Uri $Uri -OutFile $Destination
}

$toolCacheRoot = Join-Path $env:LOCALAPPDATA "ChatOS\build-tools"
$userDotnetRoot = Join-Path $toolCacheRoot "dotnet"

function Find-DotnetExecutable {
    $candidates = @(
        (Join-Path $userDotnetRoot "dotnet.exe"),
        (Join-Path $env:ProgramFiles "dotnet\dotnet.exe"),
        (Join-Path $env:LOCALAPPDATA "Microsoft\dotnet\dotnet.exe")
    )
    $candidate = $candidates | Where-Object { Test-Path $_ -PathType Leaf } | Select-Object -First 1
    if ($candidate) { return $candidate }

    $command = Get-Command dotnet.exe -ErrorAction SilentlyContinue
    if ($command) { return $command.Source }
    return $null
}

function Install-DotnetSdk {
    if ($SkipToolInstall) {
        throw ".NET 8 SDK is required. Rerun without -SkipToolInstall to install it automatically."
    }

    $null = New-Item -ItemType Directory -Path $toolCacheRoot -Force
    $dotnetInstaller = Join-Path ([IO.Path]::GetTempPath()) "chatos-dotnet-install.ps1"
    Write-Host "Downloading the Microsoft .NET 8 SDK installer..."
    Save-RemoteFile -Uri "https://dot.net/v1/dotnet-install.ps1" -Destination $dotnetInstaller
    Write-Host "Installing .NET 8 SDK for the current user..."
    $global:LASTEXITCODE = 0
    & $dotnetInstaller -Channel "8.0" -InstallDir $userDotnetRoot -NoPath
    if ($LASTEXITCODE -ne 0) {
        throw ".NET 8 SDK installation failed with exit code $LASTEXITCODE."
    }
}

$dotnetExecutable = Find-DotnetExecutable
if (-not $dotnetExecutable) {
    Install-DotnetSdk
    $dotnetExecutable = Find-DotnetExecutable
}
if (-not $dotnetExecutable) {
    throw ".NET 8 SDK installation completed but dotnet.exe was not found."
}
$dotnetDirectory = Split-Path -Parent $dotnetExecutable
$env:DOTNET_ROOT = $dotnetDirectory
$env:PATH = "$dotnetDirectory;$env:PATH"
$sdkVersions = @(& $dotnetExecutable --list-sdks)
if (-not ($sdkVersions | Where-Object { $_ -match '^8\.' })) {
    if ($SkipToolInstall) {
        throw ".NET 8 SDK is required. Installed SDKs: $($sdkVersions -join ', ')"
    }
    Install-DotnetSdk
    $dotnetExecutable = Find-DotnetExecutable
    if (-not $dotnetExecutable) {
        throw ".NET 8 SDK installation completed but dotnet.exe was not found."
    }
    $dotnetDirectory = Split-Path -Parent $dotnetExecutable
    $env:DOTNET_ROOT = $dotnetDirectory
    $env:PATH = "$dotnetDirectory;$env:PATH"
    $sdkVersions = @(& $dotnetExecutable --list-sdks)
    if (-not ($sdkVersions | Where-Object { $_ -match '^8\.' })) {
        throw ".NET 8 SDK installation did not provide an 8.x SDK. Installed SDKs: $($sdkVersions -join ', ')"
    }
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
    if ($winget) {
        Write-Host "Installing Inno Setup 6 with winget..."
        & $winget.Source install --id JRSoftware.InnoSetup --exact --silent `
            --accept-package-agreements --accept-source-agreements
        if ($LASTEXITCODE -ne 0) {
            Write-Warning "winget could not install Inno Setup; using the official installer instead."
        }
        $innoCompiler = Find-InnoSetupCompiler
    }

    if (-not $innoCompiler) {
        $innoInstaller = Join-Path ([IO.Path]::GetTempPath()) "chatos-inno-setup.exe"
        Write-Host "Downloading the official Inno Setup 6 installer..."
        Save-RemoteFile `
            -Uri "https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe" `
            -Destination $innoInstaller
        $innoInstallProcess = Start-Process `
            -FilePath $innoInstaller `
            -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER" `
            -Wait `
            -PassThru
        if ($innoInstallProcess.ExitCode -ne 0) {
            throw "Inno Setup installation failed with exit code $($innoInstallProcess.ExitCode)."
        }
        $innoCompiler = Find-InnoSetupCompiler
    }
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
    & $dotnetExecutable publish $desktopProject `
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
