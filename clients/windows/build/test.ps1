[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",

    [switch]$SkipMachineAcceptance
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repoRoot
try {
    foreach ($project in @(
        ".\tests\ChatOS.Core.Tests\ChatOS.Core.Tests.csproj",
        ".\tests\ChatOS.Api.Tests\ChatOS.Api.Tests.csproj",
        ".\tests\ChatOS.Presentation.Tests\ChatOS.Presentation.Tests.csproj",
        ".\tests\ChatOS.Connector.Tests\ChatOS.Connector.Tests.csproj",
        ".\tests\ChatOS.NetworkGuard.Tests\ChatOS.NetworkGuard.Tests.csproj")) {
        $arguments = @("test", $project, "-c", $Configuration, "--nologo")
        if ($SkipMachineAcceptance -and
            $project -eq ".\tests\ChatOS.Connector.Tests\ChatOS.Connector.Tests.csproj") {
            $arguments += @(
                "--filter",
                "Category!=WindowsNative&Category!=NetworkGuardEndToEnd")
        }
        & dotnet @arguments
        if ($LASTEXITCODE -ne 0) { throw "Test project failed: $project" }
    }
    & (Join-Path $repoRoot "build\test-networkguard-package-evidence.ps1")
    & (Join-Path $repoRoot "build\test-trx-evidence.ps1")
    & (Join-Path $repoRoot "build\test-acceptance-verifier.ps1")
}
finally {
    Pop-Location
}
