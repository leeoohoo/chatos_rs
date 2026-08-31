[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",

    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64",

    [ValidatePattern("^https?://")]
    [string]$ApiBaseUrl = "https://gateway.jgoool.com/api/chatos",

    [ValidatePattern("^https?://")]
    [string]$LocalConnectorCloudBaseUrl = "https://local-connector.jgoool.com",

    [switch]$NoBuild
)

$ErrorActionPreference = "Stop"
Write-Warning "scripts/start-server.ps1 was misnamed and is deprecated. Use scripts/start-client.ps1."
& (Join-Path $PSScriptRoot "start-client.ps1") @PSBoundParameters
