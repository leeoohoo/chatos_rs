[CmdletBinding()]
param(
    [switch]$RemoveUserData
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows) { throw "ChatOS Windows can only be uninstalled on Windows." }
$localAppData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
$installRoot = Join-Path $localAppData "Programs\ChatOS"
$startMenuShortcut = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::Programs)) "ChatOS.lnk"
$desktopShortcut = Join-Path ([Environment]::GetFolderPath([Environment+SpecialFolder]::DesktopDirectory)) "ChatOS.lnk"

Get-Process -Name "ChatOS.Desktop" -ErrorAction SilentlyContinue | Stop-Process -Force
foreach ($shortcut in @($startMenuShortcut, $desktopShortcut)) {
    Remove-Item $shortcut -Force -ErrorAction SilentlyContinue
}
if (Test-Path $installRoot) {
    Remove-Item $installRoot -Recurse -Force
}
if ($RemoveUserData) {
    $userDataRoot = Join-Path $localAppData "ChatOS"
    if (Test-Path $userDataRoot) {
        Remove-Item $userDataRoot -Recurse -Force
    }
    Write-Host "ChatOS Windows and local user data were removed."
}
else {
    Write-Host "ChatOS Windows was removed. Local settings and cached data were preserved."
}
