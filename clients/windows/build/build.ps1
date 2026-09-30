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
$rustTarget = if ($Platform -eq "ARM64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
$monorepoRoot = (Resolve-Path (Join-Path $repoRoot "..\..")).Path
$cargoArguments = @("build", "--manifest-path", (Join-Path $monorepoRoot "Cargo.toml"), "-p", "chatos_local_agent_host", "--target", $rustTarget)
if ($Configuration -eq "Release") { $cargoArguments += "--release" }
$rustProfile = if ($Configuration -eq "Release") { "release" } else { "debug" }

Push-Location $repoRoot
try {
    cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Local Agent Host build failed with exit code $LASTEXITCODE."
    }
    $localAgentHost = Join-Path $monorepoRoot "target-shared\$rustTarget\$rustProfile\chatos_local_agent_host.exe"
    if (-not (Test-Path $localAgentHost -PathType Leaf)) {
        throw "Local Agent Host executable was not produced at $localAgentHost."
    }
    dotnet build .\src\ChatOS.Desktop\ChatOS.Desktop.csproj `
        -c $Configuration `
        -p:Platform=$Platform `
        -p:RuntimeIdentifier=$runtimeIdentifier `
        -p:LocalAgentHostExecutable=$localAgentHost `
        -p:RequireLocalAgentHost=true `
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
