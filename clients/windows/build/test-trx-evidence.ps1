[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "trx-evidence.ps1")
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) "chatos-trx-evidence-$([Guid]::NewGuid().ToString('N'))"
$null = New-Item -ItemType Directory -Path $fixtureRoot -Force

function Write-TrxFixture {
    param(
        [Parameter(Mandatory)] [string]$Path,
        [Parameter(Mandatory)] [string[]]$Names,
        [string]$FailedName
    )
    $results = foreach ($name in $Names) {
        $escaped = [Security.SecurityElement]::Escape($name)
        $outcome = if ($name -eq $FailedName) { "Failed" } else { "Passed" }
        '<UnitTestResult testName="{0}" outcome="{1}" />' -f $escaped, $outcome
    }
    $failed = if ([string]::IsNullOrWhiteSpace($FailedName)) { 0 } else { 1 }
    $passed = $Names.Count - $failed
    $xml = @"
<?xml version="1.0" encoding="utf-8"?>
<TestRun xmlns="http://microsoft.com/schemas/VisualStudio/TeamTest/2010">
  <Results>$($results -join '')</Results>
  <ResultSummary outcome="Completed">
    <Counters total="$($Names.Count)" executed="$($Names.Count)" passed="$passed" failed="$failed" />
  </ResultSummary>
</TestRun>
"@
    [IO.File]::WriteAllText($Path, $xml)
}

try {
    $validPath = Join-Path $fixtureRoot "valid.trx"
    $expected = @("FirstAcceptanceTest", "SecondAcceptanceTest")
    Write-TrxFixture $validPath @(
        "ChatOS.Tests.Acceptance.FirstAcceptanceTest",
        "ChatOS.Tests.Acceptance.SecondAcceptanceTest")
    $evidence = Assert-TrxTestRun `
        -Path $validPath `
        -EvidenceRoot $fixtureRoot `
        -ExpectedTestNames $expected `
        -MinimumTestCount 2
    if ($evidence.executed_test_count -ne 2 -or
        $evidence.passed_test_count -ne 2 -or
        $evidence.failed_test_count -ne 0 -or
        [string]::IsNullOrWhiteSpace($evidence.sha256)) {
        throw "Valid TRX evidence did not produce the expected summary."
    }

    $missingRaised = $false
    try {
        Assert-TrxTestRun `
            -Path $validPath `
            -EvidenceRoot $fixtureRoot `
            -ExpectedTestNames @("MissingAcceptanceTest") | Out-Null
    }
    catch { $missingRaised = $true }
    if (-not $missingRaised) { throw "Missing expected TRX test was not rejected." }

    $failedPath = Join-Path $fixtureRoot "failed.trx"
    Write-TrxFixture $failedPath @("ChatOS.Tests.Acceptance.FailedAcceptanceTest") "ChatOS.Tests.Acceptance.FailedAcceptanceTest"
    $failedRaised = $false
    try {
        Assert-TrxTestRun -Path $failedPath -EvidenceRoot $fixtureRoot | Out-Null
    }
    catch { $failedRaised = $true }
    if (-not $failedRaised) { throw "Failed TRX test result was not rejected." }

    Write-Host "TRX evidence tests passed."
}
finally {
    Remove-Item $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}
