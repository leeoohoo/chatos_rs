[CmdletBinding()]
param(
    [string]$InputPath
)

$ErrorActionPreference = "Stop"
$windowsRoot = Split-Path -Parent $PSScriptRoot
$desktopRoot = Join-Path $windowsRoot "src\ChatOS.Desktop"

if ([string]::IsNullOrWhiteSpace($InputPath)) {
    $InputPath = Get-ChildItem `
        (Join-Path $desktopRoot "obj") `
        -Filter "input.json" `
        -File `
        -Recurse `
        -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '[\\/]Release[\\/]' } |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}

if ([string]::IsNullOrWhiteSpace($InputPath) -or -not (Test-Path $InputPath -PathType Leaf)) {
    Write-Warning "No Release XAML compiler input was found; page diagnostics were skipped."
    return
}

$compiler = Get-ChildItem `
    (Join-Path $env:USERPROFILE ".nuget\packages\microsoft.windowsappsdk") `
    -Filter "XamlCompiler.exe" `
    -File `
    -Recurse `
    -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '[\\/]tools[\\/]net472[\\/]' } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName

if ([string]::IsNullOrWhiteSpace($compiler)) {
    Write-Warning "XamlCompiler.exe was not found in the NuGet package cache."
    return
}

$sourceInput = Get-Content $InputPath -Raw | ConvertFrom-Json
$pages = @($sourceInput.XamlPages)
if ($pages.Count -eq 0) {
    Write-Warning "The XAML compiler input contains no pages."
    return
}

$sharedPages = @($pages | Where-Object { $_.ItemSpec -like "DesignSystem/*" })
$candidatePages = @($pages | Where-Object { $_.ItemSpec -notlike "DesignSystem/*" })
$diagnosticRoot = Join-Path `
    ([IO.Path]::GetTempPath()) `
    ("chatos-xaml-diagnostics-" + [Guid]::NewGuid().ToString("N"))
$null = New-Item -ItemType Directory -Path $diagnosticRoot -Force

Write-Host "The WinUI compiler returned no useful diagnostic. Checking XAML pages individually..."
Push-Location $desktopRoot
try {
    $failures = [Collections.Generic.List[string]]::new()
    foreach ($page in $candidatePages) {
        $caseName = [IO.Path]::GetFileNameWithoutExtension([string]$page.ItemSpec)
        $caseRoot = Join-Path $diagnosticRoot $caseName
        $null = New-Item -ItemType Directory -Path $caseRoot -Force

        $caseInput = Get-Content $InputPath -Raw | ConvertFrom-Json
        $caseInput.XamlPages = @($sharedPages) + @($page)
        $caseInput.OutputPath = $caseRoot + [IO.Path]::DirectorySeparatorChar
        $caseInput.SavedStateFile = Join-Path $caseRoot "XamlSaveStateFile.xml"
        $caseInputPath = Join-Path $caseRoot "input.json"
        $caseOutputPath = Join-Path $caseRoot "output.json"
        ConvertTo-Json -InputObject $caseInput -Depth 100 |
            Set-Content $caseInputPath -Encoding UTF8

        $global:LASTEXITCODE = 0
        & $compiler $caseInputPath $caseOutputPath 2>&1 | Out-Host
        if ($LASTEXITCODE -ne 0) {
            $failures.Add([string]$page.ItemSpec)
        }
    }

    if ($failures.Count -eq 0) {
        Write-Warning "Every page compiled alone; the failure is caused by an interaction between XAML pages."
    }
    else {
        Write-Warning "XAML pages that reproduce the compiler failure:"
        $failures | ForEach-Object { Write-Warning "  $_" }
    }
}
finally {
    Pop-Location
    Remove-Item $diagnosticRoot -Recurse -Force -ErrorAction SilentlyContinue
}
