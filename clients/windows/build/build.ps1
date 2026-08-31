[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",

    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not $IsWindows) {
    throw "The WinUI desktop and Windows Connector build must run on Windows. Use build/test.ps1 for cross-platform Core/API validation."
}

$runtimeIdentifier = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }

Push-Location $repoRoot
try {
    dotnet build .\src\ChatOS.Desktop\ChatOS.Desktop.csproj `
        -c $Configuration `
        -p:Platform=$Platform `
        -p:RuntimeIdentifier=$runtimeIdentifier `
        --nologo
    if ($LASTEXITCODE -ne 0) {
        throw "ChatOS Windows build failed with exit code $LASTEXITCODE."
    }

    $outputRoot = Join-Path $repoRoot "src\ChatOS.Desktop\bin\$Platform\$Configuration"
    $executable = Get-ChildItem $outputRoot -Filter "ChatOS.Desktop.exe" -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like "*$runtimeIdentifier*" } |
        Select-Object -First 1
    if (-not $executable) {
        throw "Build completed without ChatOS.Desktop.exe for $Configuration/$Platform."
    }

    Write-Host "Built: $($executable.FullName)"
}
finally {
    Pop-Location
}
