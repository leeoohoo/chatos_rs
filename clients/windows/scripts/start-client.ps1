[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",

    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64",

    [ValidatePattern("^https?://")]
    [string]$ApiBaseUrl = "https://gateway.jgoool.com/api/chatos",

    [ValidatePattern("^https?://")]
    [string]$LocalConnectorCloudBaseUrl = "https://local-connector.jgoool.com",

    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$isWindowsPlatform = if ($PSVersionTable.PSEdition -eq "Desktop") {
    $env:OS -eq "Windows_NT"
}
else {
    $IsWindows
}

if (-not $isWindowsPlatform) {
    throw "ChatOS.Desktop is a WinUI application and must be started on Windows."
}

$dotnetCommand = Get-Command dotnet -ErrorAction SilentlyContinue
if (-not $dotnetCommand) {
    throw ".NET 8 SDK is required. Install it from https://dotnet.microsoft.com/download/dotnet/8.0"
}

$sdkVersions = @(dotnet --list-sdks)
if (-not ($sdkVersions | Where-Object { $_ -match '^8\.' })) {
    throw ".NET 8 SDK is required. Installed SDKs: $($sdkVersions -join ', ')"
}

$normalizedApiBaseUrl = $ApiBaseUrl.TrimEnd('/')
$normalizedConnectorBaseUrl = $LocalConnectorCloudBaseUrl.TrimEnd('/')
$healthUrl = "$normalizedApiBaseUrl/health"
$connectorHealthUrl = "$normalizedConnectorBaseUrl/api/health"

Write-Host "Checking ChatOS server: $healthUrl"
try {
    $healthResponse = Invoke-WebRequest -Uri $healthUrl -Method Get -TimeoutSec 15 -UseBasicParsing
}
catch {
    throw "ChatOS server health check failed: $($_.Exception.GetType().Name)"
}

if ($healthResponse.StatusCode -ne 200) {
    throw "ChatOS server is not healthy. HTTP status: $($healthResponse.StatusCode)"
}

Write-Host "Checking Local Connector service: $connectorHealthUrl"
try {
    $connectorHealthResponse = Invoke-WebRequest `
        -Uri $connectorHealthUrl `
        -Method Get `
        -TimeoutSec 15 `
        -UseBasicParsing
}
catch {
    throw "Local Connector service health check failed: $($_.Exception.GetType().Name)"
}

if ($connectorHealthResponse.StatusCode -ne 200) {
    throw "Local Connector service is not healthy. HTTP status: $($connectorHealthResponse.StatusCode)"
}

$env:CHATOS_API_BASE_URL = $normalizedApiBaseUrl
$env:CHATOS_LOCAL_CONNECTOR_CLOUD_BASE_URL = $normalizedConnectorBaseUrl
$runtimeIdentifier = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
$desktopProject = Join-Path $repoRoot "src\ChatOS.Desktop\ChatOS.Desktop.csproj"

Push-Location $repoRoot
try {
    if (-not $NoBuild) {
        Write-Host "Restoring ChatOS Windows dependencies..."
        dotnet restore $desktopProject `
            -p:Platform=$Platform `
            -p:RuntimeIdentifier=$runtimeIdentifier `
            --nologo
        if ($LASTEXITCODE -ne 0) {
            throw "dotnet restore failed with exit code $LASTEXITCODE."
        }

        Write-Host "Building ChatOS Windows ($Configuration/$Platform)..."
        & (Join-Path $repoRoot "build\build.ps1") `
            -Configuration $Configuration `
            -Platform $Platform
        if ($LASTEXITCODE -ne 0) {
            throw "ChatOS Windows build failed with exit code $LASTEXITCODE."
        }
    }

    $desktopBin = Join-Path $repoRoot "src\ChatOS.Desktop\bin"
    $executable = Get-ChildItem -Path $desktopBin -Filter "ChatOS.Desktop.exe" -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object {
            $_.FullName -like "*$Configuration*" -and
            $_.FullName -like "*$runtimeIdentifier*"
        } |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1

    if (-not $executable) {
        throw "ChatOS.Desktop.exe was not found for $Configuration/$Platform. Run again without -NoBuild."
    }

    $runningProcesses = @(Get-Process -Name "ChatOS.Desktop" -ErrorAction SilentlyContinue)
    if ($runningProcesses.Count -gt 0) {
        Write-Host "Stopping the currently running ChatOS Windows client..."
        $runningProcesses | Stop-Process -Force
        Start-Sleep -Milliseconds 500
    }

    Write-Host "Starting ChatOS Windows with API: $normalizedApiBaseUrl"
    Start-Process -FilePath $executable.FullName -WorkingDirectory $executable.DirectoryName
    Write-Host "ChatOS Windows started successfully."
}
finally {
    Pop-Location
}
