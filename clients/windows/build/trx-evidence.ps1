function Assert-TrxTestRun {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Path,

        [Parameter(Mandatory)]
        [string]$EvidenceRoot,

        [string[]]$ExpectedTestNames = @(),

        [ValidateRange(1, 100000)]
        [int]$MinimumTestCount = 1
    )

    if (-not (Test-Path $Path -PathType Leaf)) {
        throw "TRX evidence file was not found: $Path"
    }
    try {
        [xml]$document = Get-Content $Path -Raw
    }
    catch {
        throw "TRX evidence is not valid XML: $Path"
    }

    $results = @($document.SelectNodes("//*[local-name()='UnitTestResult']"))
    if ($results.Count -lt $MinimumTestCount) {
        throw "TRX executed $($results.Count) tests; at least $MinimumTestCount are required."
    }
    if ($ExpectedTestNames.Count -gt 0 -and $results.Count -ne $ExpectedTestNames.Count) {
        throw "TRX executed $($results.Count) tests; exactly $($ExpectedTestNames.Count) expected tests are required."
    }

    $actualNames = @($results | ForEach-Object { [string]$_.GetAttribute("testName") })
    foreach ($expectedName in $ExpectedTestNames) {
        $matches = @($actualNames | Where-Object {
            $_ -eq $expectedName -or
            $_.EndsWith(".$expectedName", [StringComparison]::Ordinal) -or
            $_.EndsWith("::$expectedName", [StringComparison]::Ordinal)
        })
        if ($matches.Count -ne 1) {
            throw "TRX does not contain exactly one result for expected test '$expectedName'."
        }
    }

    $passedResults = @($results | Where-Object { $_.GetAttribute("outcome") -eq "Passed" })
    $failedResults = @($results | Where-Object { $_.GetAttribute("outcome") -ne "Passed" })
    if ($failedResults.Count -ne 0) {
        throw "TRX contains $($failedResults.Count) test results that did not pass."
    }

    $counters = $document.SelectSingleNode("//*[local-name()='ResultSummary']/*[local-name()='Counters']")
    if ($null -eq $counters) {
        throw "TRX does not contain ResultSummary/Counters evidence."
    }
    $counterTotal = [int]$counters.GetAttribute("total")
    $counterExecuted = [int]$counters.GetAttribute("executed")
    $counterPassed = [int]$counters.GetAttribute("passed")
    $counterFailed = [int]$counters.GetAttribute("failed")
    if ($counterTotal -ne $results.Count -or
        $counterExecuted -ne $results.Count -or
        $counterPassed -ne $passedResults.Count -or
        $counterFailed -ne $failedResults.Count) {
        throw "TRX counters do not match the contained UnitTestResult entries."
    }

    $file = Get-Item $Path
    $root = [IO.Path]::GetFullPath($EvidenceRoot).TrimEnd('\', '/')
    $rootPrefix = $root + [IO.Path]::DirectorySeparatorChar
    $fullPath = [IO.Path]::GetFullPath($file.FullName)
    if (-not $fullPath.StartsWith($rootPrefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "TRX evidence is outside the declared evidence root."
    }

    return [ordered]@{
        path = $fullPath.Substring($rootPrefix.Length).Replace('\', '/')
        length = $file.Length
        sha256 = (Get-FileHash $fullPath -Algorithm SHA256).Hash.ToLowerInvariant()
        expected_test_count = if ($ExpectedTestNames.Count -eq 0) { $null } else { $ExpectedTestNames.Count }
        minimum_test_count = $MinimumTestCount
        executed_test_count = $results.Count
        passed_test_count = $passedResults.Count
        failed_test_count = $failedResults.Count
        test_names = @($actualNames | Sort-Object)
    }
}
