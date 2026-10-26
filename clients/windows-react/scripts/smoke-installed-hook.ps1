[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$InstallerPath,

    [string]$InstallDirectory = (Join-Path ([System.IO.Path]::GetTempPath()) ("chatos-hook-nsis-smoke-" + [guid]::NewGuid().ToString("N"))),

    [string]$ReportPath = (Join-Path ([System.IO.Path]::GetTempPath()) "chatos-windows-installed-hook-smoke.json"),

    [string]$ExpectedSidecarSha256,

    [ValidateRange(5, 120)]
    [int]$HostTimeoutSeconds = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

function Assert-FileExists {
    param([Parameter(Mandatory = $true)][string]$Path, [Parameter(Mandatory = $true)][string]$Description)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "$Description is missing: $Path"
    }
}

function Read-Exact {
    param([Parameter(Mandatory = $true)]$Stream, [Parameter(Mandatory = $true)][int]$Count)
    $buffer = New-Object byte[] $Count
    $offset = 0
    while ($offset -lt $Count) {
        $read = $Stream.Read($buffer, $offset, $Count - $offset)
        if ($read -le 0) { throw "Installed Host closed its stdio stream before a complete response." }
        $offset += $read
    }
    return $buffer
}

function Send-HostRequest {
    param(
        [Parameter(Mandatory = $true)]$Process,
        [Parameter(Mandatory = $true)][hashtable]$Command,
        [Parameter(Mandatory = $true)][int]$Sequence
    )
    $request = [ordered]@{
        protocol_version = 49
        command_id = "installed-hook-smoke-$Sequence"
        command = $Command
    }
    $body = [Text.Encoding]::UTF8.GetBytes(($request | ConvertTo-Json -Depth 30 -Compress))
    $prefix = [BitConverter]::GetBytes([uint32]$body.Length)
    [Array]::Reverse($prefix)
    $stream = $Process.StandardInput.BaseStream
    $stream.Write($prefix, 0, $prefix.Length)
    $stream.Write($body, 0, $body.Length)
    $stream.Flush()
    $responsePrefix = Read-Exact -Stream $Process.StandardOutput.BaseStream -Count 4
    [Array]::Reverse($responsePrefix)
    $responseLength = [BitConverter]::ToUInt32($responsePrefix, 0)
    if ($responseLength -eq 0 -or $responseLength -gt (64 * 1024 * 1024)) {
        throw "Installed Host returned an invalid response frame length: $responseLength"
    }
    $responseBytes = Read-Exact -Stream $Process.StandardOutput.BaseStream -Count ([int]$responseLength)
    return [Text.Encoding]::UTF8.GetString($responseBytes) | ConvertFrom-Json
}

function Quote-ProcessArgument {
    param([Parameter(Mandatory = $true)][string]$Value)
    return '"' + $Value.Replace('\', '\\').Replace('"', '\"') + '"'
}

$resolvedInstaller = (Resolve-Path -LiteralPath $InstallerPath).Path
if (Test-Path -LiteralPath $InstallDirectory) {
    throw "Smoke install directory already exists; refusing to reuse it: $InstallDirectory"
}
$workRoot = "$InstallDirectory-work"
$databasePath = Join-Path $workRoot "host.sqlite"
$pluginRoot = Join-Path $workRoot "plugin"
$reportParent = Split-Path -Parent $ReportPath
if (-not [string]::IsNullOrWhiteSpace($reportParent)) {
    New-Item -ItemType Directory -Path $reportParent -Force | Out-Null
}

$report = [ordered]@{
    schemaVersion = 1
    status = "running"
    startedAtUtc = [DateTimeOffset]::UtcNow.ToString("O")
    completedAtUtc = $null
    installerName = [System.IO.Path]::GetFileName($resolvedInstaller)
    installerSha256 = (Get-FileHash -LiteralPath $resolvedInstaller -Algorithm SHA256).Hash.ToLowerInvariant()
    sidecarSha256 = $null
    installExitCode = $null
    hostHealthVerified = $false
    hookBundleStored = $false
    hookBundleActivated = $false
    hookBlockedVerified = $false
    hostExitCode = $null
    uninstallExitCode = $null
    error = $null
}

$hostProcess = $null
$uninstallerPath = $null
$failure = $null
try {
    $install = Start-Process -FilePath $resolvedInstaller -ArgumentList @('/S', "/D=$InstallDirectory") -Wait -PassThru
    $report.installExitCode = $install.ExitCode
    if ($install.ExitCode -ne 0) { throw "NSIS installer failed with exit code $($install.ExitCode)" }

    $sidecarPath = Join-Path $InstallDirectory "chatos_local_agent_host.exe"
    Assert-FileExists -Path $sidecarPath -Description "Installed Local Agent Host"
    $report.sidecarSha256 = (Get-FileHash -LiteralPath $sidecarPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if (-not [string]::IsNullOrWhiteSpace($ExpectedSidecarSha256) -and $report.sidecarSha256 -ne $ExpectedSidecarSha256.ToLowerInvariant()) {
        throw "Installed Local Agent Host hash does not match the prepared sidecar"
    }

    New-Item -ItemType Directory -Path $workRoot -Force | Out-Null
    New-Item -ItemType Directory -Path $pluginRoot -Force | Out-Null
    $hookCommandPath = Join-Path $pluginRoot "block.cmd"
    Set-Content -LiteralPath $hookCommandPath -Value "@echo off`r`necho {`"continue`":false,`"stopReason`":`"installed-hook-blocked`"}" -Encoding ASCII
    $installationRoot = (Resolve-Path -LiteralPath $pluginRoot).Path

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $sidecarPath
    $psi.Arguments = @(
        '--database', (Quote-ProcessArgument $databasePath),
        '--owner-user-id', 'user-1',
        '--disable-workers',
        '--stdio'
    ) -join ' '
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $hostProcess = New-Object System.Diagnostics.Process
    $hostProcess.StartInfo = $psi
    if (-not $hostProcess.Start()) { throw "Installed Local Agent Host failed to start." }

    $sequence = 0
    $sequence++
    $health = Send-HostRequest -Process $hostProcess -Sequence $sequence -Command @{ type = 'health' }
    if (-not $health.ok -or $health.result.type -ne 'health') { throw "Installed Host health handshake failed." }
    $report.hostHealthVerified = $true

    $bundle = [ordered]@{
        bundle_id = 'installed-hook-bundle'
        owner_user_id = 'user-1'
        plugin_id = 'plugin-installed-hook'
        release_id = 'release-1'
        component_key = 'lifecycle'
        artifact_sha256 = ('a' * 64)
        content_sha256 = ('b' * 64)
        normalized_hook_sha256 = ('c' * 64)
        command_sha256 = ('d' * 64)
        permission_snapshot_sha256 = ('e' * 64)
        runner_policy_version = 1
        installation_root = $installationRoot
        permissions = @('process.spawn')
        hooks = @(@{
            hook_id = 'prompt'
            event = 'UserPromptSubmit'
            matcher = $null
            command = 'block.cmd'
            timeout_ms = 1000
            additional_context_limit = 65536
        })
        enabled = $true
        trusted_at_unix_ms = 1
    }
    $sequence++
    $stored = Send-HostRequest -Process $hostProcess -Sequence $sequence -Command @{ type = 'put_plugin_hook_bundle'; bundle = $bundle }
    if (-not $stored.ok) { throw "Installed Host rejected Hook Bundle: $($stored.error.message)" }
    $report.hookBundleStored = $true

    $sequence++
    $active = Send-HostRequest -Process $hostProcess -Sequence $sequence -Command @{ type = 'set_active_plugin_hook_bundles'; owner_user_id = 'user-1'; bundle_ids = @('installed-hook-bundle') }
    if (-not $active.ok) { throw "Installed Host rejected Hook activation: $($active.error.message)" }
    $report.hookBundleActivated = $true

    $sequence++
    $turn = Send-HostRequest -Process $hostProcess -Sequence $sequence -Command @{
        type = 'start_conversation_turn'
        owner_user_id = 'user-1'
        conversation_id = 'missing-conversation'
        expected_conversation_version = 1
        turn_id = 'installed-hook-turn'
        message_id = 'installed-hook-message'
        run_id = 'installed-hook-run'
        message = 'installed hook smoke'
        message_metadata = @{}
        attachments = @()
        model_config_ref = 'model-1'
        model_config_revision = 'revision-1'
        capability_policy_revision = 'policy-1'
        max_iterations = 1
    }
    if ($turn.ok -or $turn.error.code -ne 'hook_blocked' -or $turn.error.message -ne 'installed-hook-blocked') {
        throw "Installed Host did not reject the prompt through the active Hook: $($turn | ConvertTo-Json -Depth 10 -Compress)"
    }
    $report.hookBlockedVerified = $true

    $hostProcess.StandardInput.Close()
    if (-not $hostProcess.WaitForExit($HostTimeoutSeconds * 1000)) {
        $hostProcess.Kill()
        $hostProcess.WaitForExit()
        throw "Installed Local Agent Host did not exit after stdio close."
    }
    $report.hostExitCode = $hostProcess.ExitCode
    $report.status = 'passed'
}
catch {
    $failure = $_
    $report.status = 'failed'
    $report.error = $_.Exception.Message
}
finally {
    if ($null -ne $hostProcess) {
        try {
            if (-not $hostProcess.HasExited) { $hostProcess.Kill(); $hostProcess.WaitForExit() }
        } catch { }
        $hostProcess.Dispose()
    }
    if (Test-Path -LiteralPath $InstallDirectory -PathType Container) {
        $uninstallers = @(Get-ChildItem -LiteralPath $InstallDirectory -File -Filter 'unins*.exe' -ErrorAction SilentlyContinue)
        if ($uninstallers.Count -eq 1) { $uninstallerPath = $uninstallers[0].FullName }
    }
    if ($null -ne $uninstallerPath -and (Test-Path -LiteralPath $uninstallerPath -PathType Leaf)) {
        try {
            $uninstall = Start-Process -FilePath $uninstallerPath -ArgumentList '/S' -Wait -PassThru
            $report.uninstallExitCode = $uninstall.ExitCode
        } catch {
            if ($null -eq $failure) { $failure = $_; $report.status = 'failed'; $report.error = $_.Exception.Message }
        }
    }
    foreach ($path in @($workRoot, $InstallDirectory)) {
        if (Test-Path -LiteralPath $path) {
            Remove-Item -LiteralPath $path -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
    $report.completedAtUtc = [DateTimeOffset]::UtcNow.ToString('O')
    $report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath $ReportPath -Encoding UTF8
    Write-Host "Windows installed Hook smoke report: $ReportPath"
}

if ($null -ne $failure) { throw $failure }
