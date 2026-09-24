[CmdletBinding()]
param(
    [ValidateSet("x64", "ARM64")]
    [string]$Platform,

    [ValidatePattern("^https?://")]
    [string]$ApiBaseUrl = "https://gateway.jgoool.com/api/chatos",

    [ValidatePattern("^https?://")]
    [string]$LocalConnectorCloudBaseUrl = "https://local-connector.jgoool.com",

    [ValidatePattern("^[0-9]+\.[0-9]+\.[0-9]+(?:\.[0-9]+)?$")]
    [string]$Version = "3.0.5",

    [switch]$SkipTests,

    [switch]$IncludeMachineAcceptanceTests,

    [switch]$SkipToolInstall,

    [switch]$Install,

    [switch]$NoLaunch
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

function Write-StartupDiagnostic {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Message
    )

    $logPath = Join-Path $env:LOCALAPPDATA "ChatOS\logs\startup.log"
    $logDirectory = Split-Path -Parent $logPath
    $null = New-Item -ItemType Directory -Path $logDirectory -Force
    $timestamp = [DateTimeOffset]::Now.ToString("O")
    Add-Content -LiteralPath $logPath -Value "[$timestamp] Installer: $Message" -Encoding utf8
}

function Write-RecentChatOSCrashEvents {
    param(
        [Parameter(Mandatory = $true)]
        [DateTime]$Since
    )

    try {
        Start-Sleep -Seconds 1
        $events = Get-WinEvent `
            -FilterHashtable @{ LogName = "Application"; StartTime = $Since } `
            -ErrorAction Stop |
            Where-Object {
                $_.Message -match "ChatOS\.Desktop" -or
                ($_.ProviderName -in @(".NET Runtime", "Application Error", "Windows Error Reporting") -and
                    $_.Message -match "ChatOS")
            } |
            Select-Object -First 5
        if (-not $events) {
            Write-StartupDiagnostic "No matching Windows Application crash event was available yet."
            return
        }
        foreach ($event in $events) {
            $eventText = ($event.Message -replace "`r?`n", " | ").Trim()
            Write-StartupDiagnostic `
                "Windows event: Provider=$($event.ProviderName); Id=$($event.Id); $eventText"
        }
    }
    catch {
        Write-StartupDiagnostic "Unable to read Windows Application events: $($_.Exception.Message)"
    }
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
        $testParameters = @{ Configuration = "Release" }
        if (-not $IncludeMachineAcceptanceTests) {
            $testParameters.SkipMachineAcceptance = $true
        }
        & (Join-Path $repoRoot "build\test.ps1") @testParameters
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

    Write-Host "Cleaning stale ChatOS Desktop build state..."
    & $dotnetExecutable clean $desktopProject `
        -c Release `
        -p:Platform=$Platform `
        -p:RuntimeIdentifier=$runtimeIdentifier `
        --nologo
    if ($LASTEXITCODE -ne 0) {
        throw "ChatOS Windows clean failed with exit code $LASTEXITCODE."
    }

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
        $publishExitCode = $LASTEXITCODE
        & (Join-Path $repoRoot "build\diagnose-xaml-compiler.ps1")
        throw "ChatOS Windows publish failed with exit code $publishExitCode."
    }

    $executable = Join-Path $payloadRoot "ChatOS.Desktop.exe"
    if (-not (Test-Path $executable -PathType Leaf)) {
        throw "Installer payload does not contain ChatOS.Desktop.exe."
    }

    $runtimeSettings = [ordered]@{
        api_base_url = $normalizedApiBaseUrl
        local_connector_cloud_base_url = $normalizedConnectorBaseUrl
    }
    [IO.File]::WriteAllText(
        (Join-Path $payloadRoot "chatos.runtime.json"),
        ($runtimeSettings | ConvertTo-Json),
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

    if ($Install) {
        Write-Host "Installing ChatOS for the current Windows user..."
        $installProcess = Start-Process `
            -FilePath $installer `
            -ArgumentList "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CURRENTUSER" `
            -Wait `
            -PassThru
        if ($installProcess.ExitCode -ne 0) {
            throw "ChatOS installer failed with exit code $($installProcess.ExitCode)."
        }

        $installedRoot = Join-Path $env:LOCALAPPDATA "Programs\ChatOS"
        $installedExecutable = Join-Path $installedRoot "ChatOS.Desktop.exe"
        if (-not (Test-Path $installedExecutable -PathType Leaf)) {
            throw "ChatOS installation completed without the expected application files."
        }

        Write-Host "ChatOS installed to: $installedRoot"
        if (-not $NoLaunch) {
            $startupLog = Join-Path $env:LOCALAPPDATA "ChatOS\logs\startup.log"
            $launchStartedAt = [DateTime]::Now
            $installedHash = (Get-FileHash $installedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
            Write-StartupDiagnostic `
                "Launch verification starting. SourceRevision=$sourceRevision; Executable=$installedExecutable; SHA256=$installedHash"
            try {
                $process = Start-Process `
                    -FilePath $installedExecutable `
                    -WorkingDirectory $installedRoot `
                    -PassThru
            }
            catch {
                Write-StartupDiagnostic "Start-Process failed: $($_.Exception)"
                throw
            }
            Write-StartupDiagnostic "Process created. PID=$($process.Id)"
            $startupDeadline = [DateTime]::UtcNow.AddSeconds(30)
            $windowDetected = $false
            while ([DateTime]::UtcNow -lt $startupDeadline) {
                Start-Sleep -Milliseconds 250
                if ($process.HasExited) {
                    Write-StartupDiagnostic `
                        "Process exited before showing a window. PID=$($process.Id); ExitCode=$($process.ExitCode)"
                    Write-RecentChatOSCrashEvents -Since $launchStartedAt
                    throw "ChatOS exited during startup with code $($process.ExitCode). See $startupLog"
                }
                $process.Refresh()
                if ($process.MainWindowHandle -ne [IntPtr]::Zero) {
                    $windowDetected = $true
                    Write-StartupDiagnostic `
                        "Main window detected. PID=$($process.Id); Handle=$($process.MainWindowHandle)"
                    break
                }
            }
            if (-not $windowDetected) {
                Write-StartupDiagnostic "No main window appeared within 30 seconds. PID=$($process.Id)"
                Write-RecentChatOSCrashEvents -Since $launchStartedAt
                throw "ChatOS did not show a window within 30 seconds. See $startupLog"
            }

            Write-Host "Verifying that the ChatOS window remains responsive..."
            $stabilityDeadline = [DateTime]::UtcNow.AddSeconds(20)
            while ([DateTime]::UtcNow -lt $stabilityDeadline) {
                Start-Sleep -Milliseconds 250
                if ($process.HasExited) {
                    Write-StartupDiagnostic `
                        "Process exited during the startup stability check. PID=$($process.Id); ExitCode=$($process.ExitCode)"
                    Write-RecentChatOSCrashEvents -Since $launchStartedAt
                    throw "ChatOS exited immediately after opening with code $($process.ExitCode). See $startupLog"
                }
            }
            $process.Refresh()
            if ($process.HasExited) {
                Write-StartupDiagnostic `
                    "Process exited at the end of the startup stability check. PID=$($process.Id); ExitCode=$($process.ExitCode)"
                Write-RecentChatOSCrashEvents -Since $launchStartedAt
                throw "ChatOS exited immediately after opening with code $($process.ExitCode). See $startupLog"
            }
            $isResponding = $process.Responding
            if (-not $isResponding) {
                Write-StartupDiagnostic "Main window stopped responding during the 20-second stability check. PID=$($process.Id)"
                Write-RecentChatOSCrashEvents -Since $launchStartedAt
                throw "ChatOS opened but stopped responding. See $startupLog"
            }
            Write-StartupDiagnostic "Startup stability check passed after 20 seconds. PID=$($process.Id); Responding=$isResponding"
            Write-Host "ChatOS started successfully (PID $($process.Id))."
        }
    }
}
finally {
    Pop-Location
}
