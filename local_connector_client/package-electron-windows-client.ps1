# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$ClientDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = Resolve-Path (Join-Path $ClientDir "..")
$FrontendDir = Join-Path $ClientDir "frontend"
$ElectronResourcesDir = Join-Path $FrontendDir "electron\resources"

function Get-CargoTargetDir {
  Push-Location $RootDir
  try {
    $metadata = cargo metadata --no-deps --format-version 1 | ConvertFrom-Json
    return $metadata.target_directory
  } finally {
    Pop-Location
  }
}

function Get-CoreBin {
  Join-Path (Get-CargoTargetDir) "release\local_connector_client_core.exe"
}

function Get-PlatformDir {
  $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
  switch ($arch) {
    "Arm64" { "windows-arm64"; break }
    "X64" { "windows-x64"; break }
    default { throw "Unsupported Windows architecture: $arch" }
  }
}

function Invoke-FrontendBuild {
  Push-Location $FrontendDir
  try {
    npm run build:electron
  } finally {
    Pop-Location
  }
}

function Invoke-CoreBuild {
  Push-Location $RootDir
  try {
    cargo build --release -p local_connector_client_core
  } finally {
    Pop-Location
  }
}

function Sync-ElectronResources {
  if (Test-Path -LiteralPath $ElectronResourcesDir) {
    $resolvedResources = Resolve-Path -LiteralPath $ElectronResourcesDir
    $resolvedFrontend = Resolve-Path -LiteralPath $FrontendDir
    if (!$resolvedResources.Path.StartsWith($resolvedFrontend.Path, [System.StringComparison]::OrdinalIgnoreCase)) {
      throw "Refusing to remove unexpected resources directory: $resolvedResources"
    }
    Remove-Item -LiteralPath $ElectronResourcesDir -Recurse -Force
  }

  New-Item -ItemType Directory -Force -Path $ElectronResourcesDir | Out-Null
  Copy-Item -LiteralPath (Get-CoreBin) -Destination (Join-Path $ElectronResourcesDir "local_connector_client_core.exe") -Force

  $platform = Get-PlatformDir
  $sourceTools = Join-Path $RootDir "bundled-tools\$platform"
  if (Test-Path -LiteralPath $sourceTools) {
    $destToolsRoot = Join-Path $ElectronResourcesDir "bundled-tools"
    New-Item -ItemType Directory -Force -Path $destToolsRoot | Out-Null
    Copy-Item -LiteralPath $sourceTools -Destination $destToolsRoot -Recurse -Force
  } else {
    Write-Warning "Bundled tools not found for $platform`: $sourceTools"
  }
}

function New-ManualElectronPackage {
  $electronDist = Join-Path $FrontendDir "node_modules\electron\dist"
  $electronExe = Join-Path $electronDist "electron.exe"
  if (!(Test-Path -LiteralPath $electronExe)) {
    throw "Electron runtime not found. Run npm install in $FrontendDir first."
  }

  $outRoot = Join-Path $ClientDir "dist\electron-windows"
  $appDir = Join-Path $outRoot "ChatOS Local Connector"
  if (Test-Path -LiteralPath $appDir) {
    $resolvedApp = Resolve-Path -LiteralPath $appDir
    $resolvedOut = Resolve-Path -LiteralPath $outRoot
    if (!$resolvedApp.Path.StartsWith($resolvedOut.Path, [System.StringComparison]::OrdinalIgnoreCase)) {
      throw "Refusing to remove unexpected Electron app directory: $resolvedApp"
    }
    Remove-Item -LiteralPath $appDir -Recurse -Force
  }

  New-Item -ItemType Directory -Force -Path $appDir | Out-Null
  Copy-Item -Path (Join-Path $electronDist "*") -Destination $appDir -Recurse -Force
  Rename-Item -LiteralPath (Join-Path $appDir "electron.exe") -NewName "ChatOS Local Connector.exe"

  $resourcesDir = Join-Path $appDir "resources"
  $appResourcesDir = Join-Path $resourcesDir "app"
  New-Item -ItemType Directory -Force -Path (Join-Path $appResourcesDir "electron") | Out-Null
  Copy-Item -LiteralPath (Join-Path $FrontendDir "dist") -Destination $appResourcesDir -Recurse -Force
  Copy-Item -LiteralPath (Join-Path $FrontendDir "electron\main.cjs") -Destination (Join-Path $appResourcesDir "electron\main.cjs") -Force
  Copy-Item -LiteralPath (Join-Path $FrontendDir "electron\preload.cjs") -Destination (Join-Path $appResourcesDir "electron\preload.cjs") -Force

  $appPackageJson = @"
{
  "name": "chatos-local-connector-desktop",
  "version": "0.1.0",
  "main": "electron/main.cjs"
}
"@
  Set-Content -LiteralPath (Join-Path $appResourcesDir "package.json") -Value $appPackageJson -Encoding ASCII

  Copy-Item -LiteralPath (Join-Path $ElectronResourcesDir "local_connector_client_core.exe") -Destination (Join-Path $resourcesDir "local_connector_client_core.exe") -Force
  Copy-Item -LiteralPath (Join-Path $ElectronResourcesDir "bundled-tools") -Destination $resourcesDir -Recurse -Force

  $zipPath = Join-Path $outRoot "ChatOS-Local-Connector-windows-x64.zip"
  if (Test-Path -LiteralPath $zipPath) {
    Remove-Item -LiteralPath $zipPath -Force
  }
  Compress-Archive -LiteralPath $appDir -DestinationPath $zipPath -Force
  Write-Host "[OK] Electron desktop app: $appDir"
  Write-Host "[OK] Electron desktop zip: $zipPath"
}

Invoke-FrontendBuild
Invoke-CoreBuild
Sync-ElectronResources
New-ManualElectronPackage

$outputDir = Join-Path $ClientDir "dist\electron-windows"
Write-Host "[OK] Electron desktop package output: $outputDir"
