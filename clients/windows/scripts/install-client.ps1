[CmdletBinding()]
param(
    [ValidateSet("x64", "ARM64")]
    [string]$Platform,

    [ValidatePattern("^https?://")]
    [string]$ApiBaseUrl = "https://gateway.jgoool.com/api/chatos",

    [ValidatePattern("^https?://")]
    [string]$LocalConnectorCloudBaseUrl = "https://local-connector.jgoool.com",

    [switch]$NoDesktopShortcut,

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
    throw "ChatOS Windows can only be installed on Windows 10/11."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$osArchitecture = [Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
if ([string]::IsNullOrWhiteSpace($Platform)) {
    $Platform = if ($osArchitecture -eq "Arm64") { "ARM64" } else { "x64" }
}
if ($Platform -eq "ARM64" -and $osArchitecture -ne "Arm64") {
    throw "ARM64 client installation requires native ARM64 Windows."
}
if ($Platform -eq "x64" -and $osArchitecture -ne "X64") {
    throw "x64 client installation requires x64 Windows."
}

$dotnetCommand = Get-Command dotnet -ErrorAction SilentlyContinue
if (-not $dotnetCommand) {
    throw ".NET 8 SDK is required to build the development client. Install it from https://dotnet.microsoft.com/download/dotnet/8.0"
}
$sdkVersions = @(dotnet --list-sdks)
if (-not ($sdkVersions | Where-Object { $_ -match '^8\.' })) {
    throw ".NET 8 SDK is required. Installed SDKs: $($sdkVersions -join ', ')"
}

$runtimeIdentifier = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
$normalizedApiBaseUrl = $ApiBaseUrl.TrimEnd('/')
$normalizedConnectorBaseUrl = $LocalConnectorCloudBaseUrl.TrimEnd('/')
$localAppData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
$installParent = Join-Path $localAppData "Programs"
$installRoot = Join-Path $installParent "ChatOS"
$stagingRoot = Join-Path $installParent "ChatOS.staging.$([Guid]::NewGuid().ToString('N'))"
$backupRoot = Join-Path $installParent "ChatOS.backup.$([Guid]::NewGuid().ToString('N'))"
$startMenuShortcut = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)) "ChatOS.lnk"
$desktopShortcut = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)) "ChatOS.lnk"
$desktopProject = Join-Path $repoRoot "src\ChatOS.Desktop\ChatOS.Desktop.csproj"
$oldInstallMoved = $false
$newInstallMoved = $false

function New-ChatOSShortcut {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string]$InstalledRoot
    )
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    $shortcut.TargetPath = (Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe")
    $shortcut.Arguments = "-NoLogo -NoProfile -ExecutionPolicy Bypass -File `"$(Join-Path $InstalledRoot 'launch.ps1')`""
    $shortcut.WorkingDirectory = $InstalledRoot
    $shortcut.IconLocation = "$(Join-Path $InstalledRoot 'ChatOS.Desktop.exe'),0"
    $shortcut.Description = "ChatOS Windows"
    $shortcut.Save()
}

$null = New-Item -ItemType Directory -Path $installParent -Force
Push-Location $repoRoot
try {
    Write-Host "Restoring ChatOS Windows dependencies..."
    dotnet restore $desktopProject `
        -p:Platform=$Platform `
        -p:RuntimeIdentifier=$runtimeIdentifier `
        --nologo
    if ($LASTEXITCODE -ne 0) { throw "ChatOS Windows restore failed." }

    Write-Host "Building ChatOS Windows Release/$Platform..."
    & (Join-Path $repoRoot "build\build.ps1") -Configuration Release -Platform $Platform

    $outputRoot = Join-Path $repoRoot "src\ChatOS.Desktop\bin\$Platform\Release"
    $executable = Get-ChildItem $outputRoot -Filter "ChatOS.Desktop.exe" -File -Recurse |
        Where-Object { $_.FullName -like "*$runtimeIdentifier*" } |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
    if (-not $executable) { throw "Release build did not produce ChatOS.Desktop.exe." }

    $runningProcesses = @(Get-Process -Name "ChatOS.Desktop" -ErrorAction SilentlyContinue)
    if ($runningProcesses.Count -gt 0) {
        Write-Host "Stopping the currently running ChatOS client..."
        $runningProcesses | Stop-Process -Force
        $runningProcesses | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
    }

    $null = New-Item -ItemType Directory -Path $stagingRoot -Force
    Copy-Item (Join-Path $executable.DirectoryName "*") $stagingRoot -Recurse -Force
    $stagedExecutable = Join-Path $stagingRoot "ChatOS.Desktop.exe"
    if (-not (Test-Path $stagedExecutable -PathType Leaf)) {
        throw "The staged installation does not contain ChatOS.Desktop.exe."
    }

    $escapedApiUrl = $normalizedApiBaseUrl.Replace("'", "''")
    $escapedConnectorUrl = $normalizedConnectorBaseUrl.Replace("'", "''")
    $launcher = @"
`$ErrorActionPreference = "Stop"
`$env:CHATOS_API_BASE_URL = '$escapedApiUrl'
`$env:CHATOS_LOCAL_CONNECTOR_CLOUD_BASE_URL = '$escapedConnectorUrl'
`$root = Split-Path -Parent `$MyInvocation.MyCommand.Path
Start-Process -FilePath (Join-Path `$root 'ChatOS.Desktop.exe') -WorkingDirectory `$root
"@
    [IO.File]::WriteAllText((Join-Path $stagingRoot "launch.ps1"), $launcher)

    $sourceRevision = $null
    if (Get-Command git -ErrorAction SilentlyContinue) {
        $sourceRevision = (& git -C $repoRoot rev-parse HEAD 2>$null | Select-Object -First 1)
    }
    $installMetadata = [ordered]@{
        schema_version = 1
        installed_at = [DateTimeOffset]::UtcNow.ToString("O")
        platform = $Platform
        runtime_identifier = $runtimeIdentifier
        api_origin = ([Uri]$normalizedApiBaseUrl).GetLeftPart([UriPartial]::Authority)
        connector_origin = ([Uri]$normalizedConnectorBaseUrl).GetLeftPart([UriPartial]::Authority)
        source_revision = if ([string]::IsNullOrWhiteSpace($sourceRevision)) { $null } else { $sourceRevision.Trim() }
        executable_sha256 = (Get-FileHash $stagedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    $installMetadata | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $stagingRoot "install-metadata.json") -Encoding utf8

    if (Test-Path $installRoot) {
        Move-Item $installRoot $backupRoot
        $oldInstallMoved = $true
    }
    Move-Item $stagingRoot $installRoot
    $newInstallMoved = $true

    New-ChatOSShortcut -Path $startMenuShortcut -InstalledRoot $installRoot
    if (-not $NoDesktopShortcut) {
        New-ChatOSShortcut -Path $desktopShortcut -InstalledRoot $installRoot
    }
    elseif (Test-Path $desktopShortcut -PathType Leaf) {
        Remove-Item $desktopShortcut -Force
    }

    if ($oldInstallMoved -and (Test-Path $backupRoot)) {
        Remove-Item $backupRoot -Recurse -Force
        $oldInstallMoved = $false
    }
    Write-Host "ChatOS Windows installed to: $installRoot"
    Write-Host "Start Menu shortcut: $startMenuShortcut"

    if (-not $NoLaunch) {
        & (Join-Path $installRoot "launch.ps1")
        Write-Host "ChatOS Windows started."
    }
}
catch {
    if ($newInstallMoved -and (Test-Path $installRoot)) {
        Remove-Item $installRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
    if ($oldInstallMoved -and (Test-Path $backupRoot)) {
        Move-Item $backupRoot $installRoot -ErrorAction SilentlyContinue
    }
    throw
}
finally {
    Pop-Location
    Remove-Item $stagingRoot -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $backupRoot -Recurse -Force -ErrorAction SilentlyContinue
}
