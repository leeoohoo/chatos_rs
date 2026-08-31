[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "networkguard-package-evidence.ps1")
$fixtureRoot = Join-Path ([IO.Path]::GetTempPath()) "chatos-networkguard-package-$([Guid]::NewGuid().ToString('N'))"
$packageRoot = Join-Path $fixtureRoot "package"
$null = New-Item -ItemType Directory -Path (Join-Path $packageRoot "driver") -Force
$null = New-Item -ItemType Directory -Path (Join-Path $packageRoot "service") -Force

function Write-Manifest {
    param([object[]]$Entries)
    $Entries | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $packageRoot "manifest.json") -Encoding utf8
}

try {
    $paths = @(
        "driver/ChatOS.NetworkGuard.Driver.sys",
        "driver/ChatOS.NetworkGuard.Driver.cat",
        "driver/ChatOS.NetworkGuard.Driver.inf",
        "service/ChatOS.NetworkGuard.Service.exe")
    $entries = foreach ($relativePath in $paths) {
        $path = Join-Path $packageRoot $relativePath
        [IO.File]::WriteAllBytes($path, [Text.Encoding]::UTF8.GetBytes($relativePath))
        $file = Get-Item $path
        [ordered]@{
            path = $relativePath
            length = $file.Length
            sha256 = (Get-FileHash $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    Write-Manifest $entries
    $validated = @(Assert-NetworkGuardPackageManifest $packageRoot)
    if ($validated.Count -ne 4) { throw "Valid package manifest did not return four entries." }

    [IO.File]::WriteAllBytes(
        (Join-Path $packageRoot "driver/ChatOS.NetworkGuard.Driver.sys"),
        [byte[]](1, 2, 3))
    $tamperRaised = $false
    try { Assert-NetworkGuardPackageManifest $packageRoot | Out-Null }
    catch { $tamperRaised = $true }
    if (-not $tamperRaised) { throw "Tampered package artifact was not rejected." }

    $entries[0].path = "../escaped.sys"
    Write-Manifest $entries
    $escapeRaised = $false
    try { Assert-NetworkGuardPackageManifest $packageRoot | Out-Null }
    catch { $escapeRaised = $true }
    if (-not $escapeRaised) { throw "Escaped manifest path was not rejected." }

    Write-Host "NetworkGuard package evidence tests passed."
}
finally {
    Remove-Item $fixtureRoot -Recurse -Force -ErrorAction SilentlyContinue
}
