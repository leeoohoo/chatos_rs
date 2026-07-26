# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [string]$DestinationDir,

  [Parameter(Mandatory = $true)]
  [ValidateSet("windows-x64", "windows-arm64")]
  [string]$Platform
)

$ErrorActionPreference = "Stop"
$FontUrl = if ($env:CHATOS_DOCUMENT_FONT_URL) {
  $env:CHATOS_DOCUMENT_FONT_URL
} else {
  "https://fonts.gstatic.com/s/notosanssc/v40/k3kCo84MPvpLmixcA63oeAL7Iqp5IZJF9bmaG9_FnYw.ttf"
}
$FontSha256 = if ($env:CHATOS_DOCUMENT_FONT_SHA256) {
  $env:CHATOS_DOCUMENT_FONT_SHA256.ToLowerInvariant()
} else {
  "450625c8d46ab3df97b7904ded955ec2746d17ec76740cb1e91d1ba63a0f89af"
}
$FontCacheRoot = if ($env:CHATOS_DOCUMENT_RUNTIME_CACHE) {
  $env:CHATOS_DOCUMENT_RUNTIME_CACHE
} else {
  Join-Path $env:LOCALAPPDATA "ChatOS\document-runtime-cache"
}
$FontLicense = Join-Path $PSScriptRoot "runtime_assets\fonts\NotoSansSC-OFL.txt"
$SourceRoot = $env:CHATOS_DOCUMENT_RUNTIME_SOURCE
if (!$SourceRoot) {
  throw "CHATOS_DOCUMENT_RUNTIME_SOURCE must point to a verified LibreOffice and Poppler runtime root"
}
if (!(Test-Path -LiteralPath $SourceRoot -PathType Container)) {
  throw "Document runtime source must be a directory: $SourceRoot"
}
if (((Get-Item -LiteralPath $SourceRoot).Attributes -band [System.IO.FileAttributes]::ReparsePoint) -ne 0) {
  throw "Document runtime source must not be a symlink or junction: $SourceRoot"
}

$LibreOfficeSource = if (Test-Path -LiteralPath (Join-Path $SourceRoot "libreoffice\program\soffice.exe")) {
  Join-Path $SourceRoot "libreoffice"
} elseif (Test-Path -LiteralPath (Join-Path $SourceRoot "LibreOffice\program\soffice.exe")) {
  Join-Path $SourceRoot "LibreOffice"
} else {
  throw "Document runtime source is missing LibreOffice program\soffice.exe"
}

$PopplerSource = Join-Path $SourceRoot "poppler"
$PopplerRelative = if (Test-Path -LiteralPath (Join-Path $PopplerSource "Library\bin\pdftoppm.exe")) {
  "poppler/Library/bin/pdftoppm.exe"
} elseif (Test-Path -LiteralPath (Join-Path $PopplerSource "bin\pdftoppm.exe")) {
  "poppler/bin/pdftoppm.exe"
} else {
  throw "Document runtime source is missing Poppler pdftoppm.exe"
}

if (Test-Path -LiteralPath $DestinationDir) {
  Remove-Item -LiteralPath $DestinationDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $DestinationDir | Out-Null
Copy-Item -LiteralPath $LibreOfficeSource -Destination (Join-Path $DestinationDir "libreoffice") -Recurse -Force
Copy-Item -LiteralPath $PopplerSource -Destination (Join-Path $DestinationDir "poppler") -Recurse -Force
New-Item -ItemType Directory -Force -Path $FontCacheRoot, (Join-Path $DestinationDir "fonts") | Out-Null
$FontCacheFile = Join-Path $FontCacheRoot "NotoSansSC-Regular.ttf"
if (!(Test-Path -LiteralPath $FontCacheFile) -or (Get-FileHash -LiteralPath $FontCacheFile -Algorithm SHA256).Hash.ToLowerInvariant() -ne $FontSha256) {
  Remove-Item -LiteralPath $FontCacheFile -Force -ErrorAction SilentlyContinue
  Write-Host "[INFO] Downloading Noto Sans SC document fallback font"
  Invoke-WebRequest -Uri $FontUrl -OutFile "$FontCacheFile.partial"
  if ((Get-FileHash -LiteralPath "$FontCacheFile.partial" -Algorithm SHA256).Hash.ToLowerInvariant() -ne $FontSha256) {
    Remove-Item -LiteralPath "$FontCacheFile.partial" -Force -ErrorAction SilentlyContinue
    throw "Downloaded document fallback font hash does not match the pinned value"
  }
  Move-Item -LiteralPath "$FontCacheFile.partial" -Destination $FontCacheFile -Force
}
Copy-Item -LiteralPath $FontCacheFile -Destination (Join-Path $DestinationDir "fonts\NotoSansSC-Regular.ttf") -Force
Copy-Item -LiteralPath $FontLicense -Destination (Join-Path $DestinationDir "fonts\NotoSansSC-OFL.txt") -Force

$SofficeRelative = "libreoffice/program/soffice.exe"
$Soffice = Join-Path $DestinationDir ($SofficeRelative.Replace("/", "\"))
$Pdftoppm = Join-Path $DestinationDir ($PopplerRelative.Replace("/", "\"))
if (!(Test-Path -LiteralPath $Soffice -PathType Leaf) -or !(Test-Path -LiteralPath $Pdftoppm -PathType Leaf)) {
  throw "Packaged document runtime is incomplete under $DestinationDir"
}

$SofficeVersion = (& $Soffice --version | Select-Object -First 1).Trim()
$PdftoppmVersion = ((& $Pdftoppm -v 2>&1 | Select-Object -First 1).ToString()).Trim()
if ($SofficeVersion -notlike "*LibreOffice*" -or $PdftoppmVersion -notlike "*pdftoppm version*") {
  throw "Packaged document runtime version probe failed"
}
$RuntimeRevision = if ($env:CHATOS_DOCUMENT_RUNTIME_REVISION) {
  $env:CHATOS_DOCUMENT_RUNTIME_REVISION
} else {
  "libreoffice-poppler-2026-07-25.1"
}
$Manifest = [ordered]@{
  schema_version = 1
  runtime_revision = $RuntimeRevision
  platform = $Platform
  soffice = [ordered]@{
    path = $SofficeRelative
    sha256 = (Get-FileHash -LiteralPath $Soffice -Algorithm SHA256).Hash.ToLowerInvariant()
    version = $SofficeVersion
  }
  pdftoppm = [ordered]@{
    path = $PopplerRelative
    sha256 = (Get-FileHash -LiteralPath $Pdftoppm -Algorithm SHA256).Hash.ToLowerInvariant()
    version = $PdftoppmVersion
  }
  poppler_library_dir = $null
  font_directory = "fonts"
  fonts = @(
    [ordered]@{
      path = "fonts/NotoSansSC-Regular.ttf"
      sha256 = $FontSha256
    }
  )
}
$ManifestPath = Join-Path $DestinationDir "runtime.json"
$ManifestJson = $Manifest | ConvertTo-Json -Depth 5
$Utf8WithoutBom = New-Object System.Text.UTF8Encoding($false)
[System.IO.File]::WriteAllText($ManifestPath, "$ManifestJson`n", $Utf8WithoutBom)

Write-Host "[OK] Document runtime: $SofficeVersion"
Write-Host "[OK] Document runtime: $PdftoppmVersion"
Write-Host "[OK] Document runtime manifest: $ManifestPath"
