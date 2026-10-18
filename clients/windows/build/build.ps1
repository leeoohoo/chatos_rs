[CmdletBinding()]
param(
    [ValidateSet("Debug", "Release")]
    [string]$Configuration = "Debug",

    [ValidateSet("x64", "ARM64")]
    [string]$Platform = "x64"
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

if (-not $IsWindows) {
    throw "The WinUI desktop and Windows Connector build must run on Windows. Use build/test.ps1 for cross-platform Core/API validation."
}

$runtimeIdentifier = if ($Platform -eq "ARM64") { "win-arm64" } else { "win-x64" }
$rustTarget = if ($Platform -eq "ARM64") { "aarch64-pc-windows-msvc" } else { "x86_64-pc-windows-msvc" }
$monorepoRoot = (Resolve-Path (Join-Path $repoRoot "..\..")).Path
$desktopProject = Join-Path $repoRoot "src\ChatOS.Desktop\ChatOS.Desktop.csproj"
$cargoArguments = @("build", "--manifest-path", (Join-Path $monorepoRoot "Cargo.toml"), "-p", "chatos_local_agent_host", "--target", $rustTarget)
if ($Configuration -eq "Release") { $cargoArguments += "--release" }
$rustProfile = if ($Configuration -eq "Release") { "release" } else { "debug" }
$previousAwsLcNoAsm = $env:AWS_LC_SYS_NO_ASM
$previousAwsLcPrebuiltNasm = $env:AWS_LC_SYS_PREBUILT_NASM
$previousCMake = $env:CMAKE

# aws-lc prefers NASM on x64, but a normal Visual Studio installation does not
# necessarily put NASM or its bundled CMake on PATH. Fall back to aws-lc's CMake
# build and discover the Visual Studio copy so a clean Windows checkout builds
# without a separate NASM installation.
if ($Configuration -eq "Release") {
    # aws-lc-sys rejects NO_ASM for optimized builds. Its checked-in x64
    # objects are the supported fallback when NASM is not installed.
    $env:AWS_LC_SYS_NO_ASM = $null
    if ($Platform -eq "x64" -and -not (Get-Command nasm -ErrorAction SilentlyContinue)) {
        $env:AWS_LC_SYS_PREBUILT_NASM = "1"
    }
}
elseif (-not (Get-Command nasm -ErrorAction SilentlyContinue)) {
    $env:AWS_LC_SYS_NO_ASM = "1"
}

if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vsWhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vsWhere -PathType Leaf) {
        $visualStudioRoot = & $vsWhere -latest -products * -property installationPath
        if ($visualStudioRoot) {
            $visualStudioCMake = Join-Path $visualStudioRoot "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin\cmake.exe"
            if (Test-Path $visualStudioCMake -PathType Leaf) {
                $env:CMAKE = $visualStudioCMake
            }
        }
    }
}

Push-Location $monorepoRoot
try {
    cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Local Agent Host build failed with exit code $LASTEXITCODE."
    }
    $localAgentHost = Join-Path $monorepoRoot "target-shared\$rustTarget\$rustProfile\chatos_local_agent_host.exe"
    if (-not (Test-Path $localAgentHost -PathType Leaf)) {
        throw "Local Agent Host executable was not produced at $localAgentHost."
    }
    dotnet build $desktopProject `
        -c $Configuration `
        -p:Platform=$Platform `
        -p:RuntimeIdentifier=$runtimeIdentifier `
        -p:LocalAgentHostExecutable=$localAgentHost `
        -p:RequireLocalAgentHost=true `
        --nologo
    if ($LASTEXITCODE -ne 0) {
        throw "ChatOS Windows build failed with exit code $LASTEXITCODE."
    }

    $outputRoot = Join-Path $repoRoot "src\ChatOS.Desktop\bin\$Platform\$Configuration"
    $executable = Get-ChildItem $outputRoot -Filter "ChatOS.Desktop.exe" -File -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -like "*$runtimeIdentifier*" } |
        Select-Object -First 1
    if (-not $executable) {
        throw "Build completed without ChatOS.Desktop.exe for $Configuration/$Platform."
    }

    Write-Host "Built: $($executable.FullName)"
}
finally {
    Pop-Location
    $env:AWS_LC_SYS_NO_ASM = $previousAwsLcNoAsm
    $env:AWS_LC_SYS_PREBUILT_NASM = $previousAwsLcPrebuiltNasm
    $env:CMAKE = $previousCMake
}
