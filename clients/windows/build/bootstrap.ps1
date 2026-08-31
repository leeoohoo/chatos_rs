[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not (Get-Command dotnet -ErrorAction SilentlyContinue)) {
    throw ".NET 8 SDK is required. Install it from https://dotnet.microsoft.com/download/dotnet/8.0"
}

$sdkVersion = dotnet --version
if (-not $sdkVersion.StartsWith("8.")) {
    throw "ChatOS Windows requires .NET 8 SDK. Current version: $sdkVersion"
}

if (-not $IsWindows) {
    Write-Warning "Core and API can be tested here, but WinUI, Connector, ConPTY, Credential Manager and MSIX require Windows."
}

Push-Location $repoRoot
try {
    foreach ($project in @(
        ".\tests\ChatOS.Core.Tests\ChatOS.Core.Tests.csproj",
        ".\tests\ChatOS.Api.Tests\ChatOS.Api.Tests.csproj",
        ".\tests\ChatOS.Presentation.Tests\ChatOS.Presentation.Tests.csproj",
        ".\tests\ChatOS.Connector.Tests\ChatOS.Connector.Tests.csproj",
        ".\tests\ChatOS.NetworkGuard.Tests\ChatOS.NetworkGuard.Tests.csproj")) {
        dotnet restore $project
        if ($LASTEXITCODE -ne 0) { throw "Restore failed: $project" }
    }

    if ($IsWindows) {
        dotnet restore .\ChatOS.Win.sln -p:Platform=x64
        if ($LASTEXITCODE -ne 0) { throw "Windows solution restore failed." }
    }
}
finally {
    Pop-Location
}
