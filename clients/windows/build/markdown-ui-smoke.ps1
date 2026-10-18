[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string]$OutputDirectory,
    [switch]$NoBuild
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'The isolated Markdown UI smoke requires Windows.' }
$windowsRoot = Split-Path $PSScriptRoot -Parent
$project = Join-Path $windowsRoot 'tests\ChatOS.Markdown.UiSmoke\ChatOS.Markdown.UiSmoke.csproj'
$executable = Join-Path $windowsRoot 'tests\ChatOS.Markdown.UiSmoke\bin\x64\Debug\net8.0-windows10.0.19041.0\win-x64\ChatOS.Markdown.UiSmoke.exe'
$resolvedOutput = [IO.Path]::GetFullPath($OutputDirectory)
if (-not $NoBuild) {
    & dotnet build $project -c Debug -p:Platform=x64 -r win-x64 --nologo
    if ($LASTEXITCODE -ne 0) { throw 'Isolated Markdown UI smoke build failed.' }
}
$runId = [Guid]::NewGuid().ToString('N')
$process = Start-Process -FilePath $executable -ArgumentList @(('"{0}"' -f $resolvedOutput), $runId) -WindowStyle Hidden -PassThru
try {
    if (-not $process.WaitForExit(30000)) { throw 'Isolated Markdown UI smoke timed out.' }
    if ($process.ExitCode -ne 0) { throw "Isolated Markdown UI smoke failed. See $resolvedOutput." }
    $result = Get-Content -LiteralPath (Join-Path $resolvedOutput 'result.json') -Raw | ConvertFrom-Json
    if ($result.runId -ne $runId -or $result.passed -ne 17 -or $result.failed -ne 0 -or $result.skipped -ne 0) { throw 'Unexpected or stale Markdown UI smoke result.' }
    foreach ($screenshot in $result.screenshots) {
        if (-not (Test-Path -LiteralPath (Join-Path $resolvedOutput $screenshot) -PathType Leaf)) {
            throw "Missing Markdown screenshot: $screenshot"
        }
    }
    Write-Host 'Isolated native Markdown UI smoke passed: 17 assertions, 3 screenshots. Authenticated pages and clipboard are not covered.'
}
finally {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
}
