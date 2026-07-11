# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidatePattern('^[A-Za-z0-9._-]+$')]
  [string]$Version,

  [string]$ArtifactPath,
  [string]$WebsiteApiBase = $env:OFFICIAL_WEBSITE_API_BASE,
  [string]$UploadToken = $env:OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN,
  [string]$Platform = "windows-x64",
  [string]$Label = "Windows 10/11 (64-bit)"
)

$ErrorActionPreference = "Stop"

$ClientDir = Split-Path -Parent $MyInvocation.MyCommand.Path
if ([string]::IsNullOrWhiteSpace($ArtifactPath)) {
  $ArtifactPath = Join-Path $ClientDir "dist\electron-windows\ChatOS-Local-Connector-windows-x64.zip"
}
if ([string]::IsNullOrWhiteSpace($WebsiteApiBase)) {
  $WebsiteApiBase = "http://127.0.0.1:39250"
}
if ([string]::IsNullOrWhiteSpace($UploadToken)) {
  throw "OFFICIAL_WEBSITE_RELEASE_UPLOAD_TOKEN or -UploadToken is required"
}

$artifact = Resolve-Path -LiteralPath $ArtifactPath
$artifactItem = Get-Item -LiteralPath $artifact.Path
$sha256 = (Get-FileHash -LiteralPath $artifact.Path -Algorithm SHA256).Hash.ToLowerInvariant()
$apiBase = $WebsiteApiBase.TrimEnd('/')

$requestBody = @{
  version = $Version
  artifacts = @(
    @{
      platform = $Platform
      label = $Label
      file_name = $artifactItem.Name
      content_type = "application/zip"
      size_bytes = $artifactItem.Length
      sha256 = $sha256
    }
  )
} | ConvertTo-Json -Depth 6

Write-Host "[INFO] Requesting MinIO upload URLs for version $Version"
$headers = @{ Authorization = "Bearer $UploadToken" }
$presign = Invoke-RestMethod `
  -Method Post `
  -Uri "$apiBase/api/site/admin/releases/presign" `
  -Headers $headers `
  -ContentType "application/json" `
  -Body $requestBody

$artifactUpload = $presign.artifact_uploads | Where-Object { $_.platform -eq $Platform } | Select-Object -First 1
if ($null -eq $artifactUpload) {
  throw "Website API did not return an upload URL for platform $Platform"
}

Write-Host "[INFO] Uploading $($artifactItem.Name) to MinIO"
Invoke-WebRequest `
  -Method Put `
  -Uri $artifactUpload.upload_url `
  -InFile $artifact.Path `
  -ContentType "application/zip" `
  -UseBasicParsing | Out-Null

$manifestJson = $presign.manifest | ConvertTo-Json -Depth 10
$manifestBytes = [System.Text.Encoding]::UTF8.GetBytes($manifestJson)
Write-Host "[INFO] Publishing stable release manifest"
Invoke-WebRequest `
  -Method Put `
  -Uri $presign.manifest_upload.upload_url `
  -Body $manifestBytes `
  -ContentType "application/json; charset=utf-8" `
  -UseBasicParsing | Out-Null

Write-Host "[OK] Published ChatOS Local Connector $Version"
Write-Host "[OK] Artifact SHA-256: $sha256"
Write-Host "[OK] Download catalog: $apiBase/api/site/downloads"
