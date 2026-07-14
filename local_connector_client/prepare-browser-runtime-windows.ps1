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
$AgentBrowserVersion = if ($env:CHATOS_AGENT_BROWSER_VERSION) { $env:CHATOS_AGENT_BROWSER_VERSION } else { "0.31.2" }
$ChromeVersion = if ($env:CHATOS_CHROME_FOR_TESTING_VERSION) { $env:CHATOS_CHROME_FOR_TESTING_VERSION } else { "150.0.7871.115" }
$CacheRoot = if ($env:CHATOS_BROWSER_RUNTIME_CACHE) {
  $env:CHATOS_BROWSER_RUNTIME_CACHE
} else {
  Join-Path $env:LOCALAPPDATA "ChatOS\browser-runtime-cache"
}

New-Item -ItemType Directory -Force -Path $CacheRoot, $DestinationDir | Out-Null
$WorkDir = Join-Path ([System.IO.Path]::GetTempPath()) ("chatos-browser-runtime-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null

try {
  $Tarball = Join-Path $CacheRoot "agent-browser-$AgentBrowserVersion.tgz"
  if (!(Test-Path -LiteralPath $Tarball)) {
    Write-Host "[INFO] Downloading agent-browser $AgentBrowserVersion"
    npm pack "agent-browser@$AgentBrowserVersion" --pack-destination $CacheRoot --silent | Out-Null
  }

  tar -xzf $Tarball -C $WorkDir "package/bin/agent-browser-win32-x64.exe" "package/LICENSE" "package/package.json"
  if ($LASTEXITCODE -ne 0) {
    throw "Unable to extract agent-browser package: $Tarball"
  }
  $Package = Get-Content -LiteralPath (Join-Path $WorkDir "package\package.json") -Raw | ConvertFrom-Json
  if ($Package.version -ne $AgentBrowserVersion) {
    throw "agent-browser package version mismatch: expected $AgentBrowserVersion, got $($Package.version)"
  }
  Copy-Item -LiteralPath (Join-Path $WorkDir "package\bin\agent-browser-win32-x64.exe") -Destination (Join-Path $DestinationDir "agent-browser.exe") -Force
  Copy-Item -LiteralPath (Join-Path $WorkDir "package\LICENSE") -Destination (Join-Path $DestinationDir "agent-browser.LICENSE") -Force

  # agent-browser currently publishes a Windows x64 runtime. Windows on ARM64
  # runs this binary and Chrome for Testing through the system x64 emulation.
  $ChromePlatform = "win64"
  $ChromeArchive = Join-Path $CacheRoot "chrome-$ChromeVersion-$ChromePlatform.zip"
  if (!(Test-Path -LiteralPath $ChromeArchive)) {
    $ChromeUrl = "https://storage.googleapis.com/chrome-for-testing-public/$ChromeVersion/$ChromePlatform/chrome-$ChromePlatform.zip"
    Write-Host "[INFO] Downloading Chrome for Testing $ChromeVersion ($ChromePlatform)"
    Invoke-WebRequest -Uri $ChromeUrl -OutFile "$ChromeArchive.partial"
    Move-Item -LiteralPath "$ChromeArchive.partial" -Destination $ChromeArchive -Force
  }

  $ChromeCacheDir = Join-Path $CacheRoot "chrome-$ChromeVersion-$ChromePlatform"
  $ChromeCacheExe = Join-Path $ChromeCacheDir "chrome-win64\chrome.exe"
  if (!(Test-Path -LiteralPath $ChromeCacheExe)) {
    if (Test-Path -LiteralPath $ChromeCacheDir) {
      Remove-Item -LiteralPath $ChromeCacheDir -Recurse -Force
    }
    New-Item -ItemType Directory -Force -Path $ChromeCacheDir | Out-Null
    Expand-Archive -LiteralPath $ChromeArchive -DestinationPath $ChromeCacheDir -Force
  }

  $BrowserDir = Join-Path $DestinationDir "browser"
  if (Test-Path -LiteralPath $BrowserDir) {
    Remove-Item -LiteralPath $BrowserDir -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $BrowserDir | Out-Null
  Copy-Item -LiteralPath (Join-Path $ChromeCacheDir "chrome-win64") -Destination $BrowserDir -Recurse -Force

  $AgentBrowserBin = Join-Path $DestinationDir "agent-browser.exe"
  $ChromeBin = Join-Path $BrowserDir "chrome-win64\chrome.exe"
  if (!(Test-Path -LiteralPath $AgentBrowserBin) -or !(Test-Path -LiteralPath $ChromeBin)) {
    throw "Packaged browser runtime is incomplete under $DestinationDir"
  }

  $AgentVersionOutput = & $AgentBrowserBin --version
  $ChromeVersionOutput = & $ChromeBin --version
  if ($AgentVersionOutput -notlike "*$AgentBrowserVersion*") {
    throw "Unexpected agent-browser version: $AgentVersionOutput"
  }
  if ($ChromeVersionOutput -notlike "*$ChromeVersion*") {
    throw "Unexpected Chrome for Testing version: $ChromeVersionOutput"
  }

  Write-Host "[OK] Browser runtime: agent-browser $AgentBrowserVersion"
  Write-Host "[OK] Browser runtime: Chrome for Testing $ChromeVersion"
} finally {
  if (Test-Path -LiteralPath $WorkDir) {
    Remove-Item -LiteralPath $WorkDir -Recurse -Force
  }
}
