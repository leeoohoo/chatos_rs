[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Release",

    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64",

    [string]$CertificateThumbprint
)

$ErrorActionPreference = "Stop"
if (-not $IsWindows) { throw "NetworkGuard must be built on Windows with Visual Studio 2022 and the WDK." }
if (-not [string]::IsNullOrWhiteSpace($CertificateThumbprint)) {
    $CertificateThumbprint = $CertificateThumbprint.Replace(" ", "").ToUpperInvariant()
    if ($CertificateThumbprint -notmatch '^[A-F0-9]{40}$') {
        throw "CertificateThumbprint must be a 40-character SHA-1 certificate thumbprint."
    }
}
$repoRoot = Split-Path -Parent $PSScriptRoot
$runtime = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
$output = Join-Path $repoRoot "artifacts\networkguard\$Platform"
if (Test-Path $output) { Remove-Item $output -Recurse -Force }
$null = New-Item $output -ItemType Directory -Force

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "Visual Studio Installer (vswhere.exe) was not found." }
$visualCppComponents = @("Microsoft.VisualStudio.Component.VC.Tools.x86.x64")
if ($Platform -eq "ARM64") {
    $visualCppComponents += "Microsoft.VisualStudio.Component.VC.Tools.ARM64"
}
$visualStudio = & $vswhere -latest -products * -requires $visualCppComponents -property installationPath |
    Select-Object -First 1
if (-not $visualStudio) {
    throw "Visual Studio C++ build tools for $Platform were not found."
}
$msbuild = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -find MSBuild\**\Bin\MSBuild.exe | Select-Object -First 1
if (-not $msbuild) { throw "Visual Studio C++ build tools were not found." }
$kitsRoot = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots" -ErrorAction Stop).KitsRoot10
$wdkProps = Get-ChildItem (Join-Path $kitsRoot "build") `
    -Filter WindowsDriver.KernelModeDriver.props -File -Recurse -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending |
    Select-Object -First 1
if (-not $wdkProps) {
    throw "The Windows Driver Kit MSBuild integration was not found."
}
$wdkVersion = $wdkProps.Directory.Name

Push-Location $repoRoot
try {
    & $msbuild .\native\ChatOS.NetworkGuard.Driver\ChatOS.NetworkGuard.Driver.vcxproj `
        /m /t:Build /p:Configuration=$Configuration /p:Platform=$Platform /nologo
    if ($LASTEXITCODE -ne 0) { throw "NetworkGuard driver build failed." }

    dotnet publish .\src\ChatOS.NetworkGuard.Service\ChatOS.NetworkGuard.Service.csproj `
        -c $Configuration -r $runtime --self-contained true `
        -p:PublishSingleFile=true -p:DebugType=None -p:DebugSymbols=false `
        -o (Join-Path $output "service") --nologo
    if ($LASTEXITCODE -ne 0) { throw "NetworkGuard service publish failed." }

    $driver = Get-ChildItem .\native\ChatOS.NetworkGuard.Driver -Filter ChatOS.NetworkGuard.Driver.sys -File -Recurse |
        Where-Object { $_.FullName -like "*$Platform*$Configuration*" } |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    $catalog = Get-ChildItem .\native\ChatOS.NetworkGuard.Driver -Filter ChatOS.NetworkGuard.Driver.cat -File -Recurse |
        Where-Object { $_.FullName -like "*$Platform*$Configuration*" } |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (-not $driver -or -not $catalog) { throw "WDK build did not produce the SYS and catalog files." }

    $driverOutput = Join-Path $output "driver"
    $null = New-Item $driverOutput -ItemType Directory -Force
    $packagedDriver = Join-Path $driverOutput "ChatOS.NetworkGuard.Driver.sys"
    $packagedCatalog = Join-Path $driverOutput "ChatOS.NetworkGuard.Driver.cat"
    Copy-Item $driver.FullName $packagedDriver
    Copy-Item .\native\ChatOS.NetworkGuard.Driver\ChatOS.NetworkGuard.Driver.inf $driverOutput

    if ($CertificateThumbprint) {
        $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Filter signtool.exe -File -Recurse |
            Where-Object { $_.FullName -like "*\x64\signtool.exe" } |
            Sort-Object FullName -Descending | Select-Object -First 1
        if (-not $signtool) { throw "signtool.exe was not found." }
        & $signtool.FullName sign /sha1 $CertificateThumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $packagedDriver
        if ($LASTEXITCODE -ne 0) { throw "NetworkGuard SYS signing failed." }

        $inf2cat = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin" -Filter Inf2Cat.exe -File -Recurse |
            Sort-Object FullName -Descending | Select-Object -First 1
        if (-not $inf2cat) { throw "Inf2Cat.exe was not found." }
        $inf2catOs = if ($Platform -eq "ARM64") { "10_ARM64" } else { "10_X64" }
        & $inf2cat.FullName "/driver:$driverOutput" "/os:$inf2catOs"
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $packagedCatalog)) {
            throw "NetworkGuard catalog regeneration failed after SYS signing."
        }
        & $signtool.FullName sign /sha1 $CertificateThumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $packagedCatalog
        if ($LASTEXITCODE -ne 0) { throw "NetworkGuard catalog signing failed." }
    }
    else {
        Copy-Item $catalog.FullName $packagedCatalog
    }

    $manifest = Get-ChildItem $output -File -Recurse | ForEach-Object {
        [ordered]@{
            path = $_.FullName.Substring($output.Length + 1).Replace('\', '/')
            length = $_.Length
            sha256 = (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    $manifest | ConvertTo-Json -Depth 4 | Set-Content (Join-Path $output "manifest.json") -Encoding utf8
    $buildReport = [ordered]@{
        schema_version = 2
        passed = $true
        built_at = [DateTimeOffset]::UtcNow.ToString("O")
        source_revision = if ([string]::IsNullOrWhiteSpace($env:GITHUB_SHA)) { $null } else { $env:GITHUB_SHA }
        configuration = $Configuration
        platform = $Platform
        wdk_version = $wdkVersion
        visual_studio = Split-Path $visualStudio -Leaf
        signed = -not [string]::IsNullOrWhiteSpace($CertificateThumbprint)
        signing_mode = if ([string]::IsNullOrWhiteSpace($CertificateThumbprint)) { "unsigned" } else { "local_test" }
        driver_signature_status = (Get-AuthenticodeSignature $packagedDriver).Status.ToString()
        catalog_signature_status = (Get-AuthenticodeSignature $packagedCatalog).Status.ToString()
        driver_signer_subject = (Get-AuthenticodeSignature $packagedDriver).SignerCertificate.Subject
        catalog_signer_subject = (Get-AuthenticodeSignature $packagedCatalog).SignerCertificate.Subject
        artifacts = $manifest
    }
    $buildReport | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $output "build-report.json") -Encoding utf8
}
finally { Pop-Location }

Write-Host "NetworkGuard package: $output"
