[CmdletBinding()]
param(
  [ValidateSet('restart', 'start', 'stop', 'status', 'bootstrap')]
  [string]$Action = 'status',
  [ValidateSet('main', 'user-service', 'task-runner', 'memory-engine', 'all')]
  [string]$Target = 'main',
  [string]$Distro
)

$ErrorActionPreference = 'Stop'

function Import-DotEnvFile {
  param([string]$Path)

  if (-not (Test-Path -LiteralPath $Path)) {
    return
  }

  foreach ($line in Get-Content -LiteralPath $Path -Encoding utf8) {
    $trimmed = $line.Trim()
    if (-not $trimmed -or $trimmed.StartsWith('#')) {
      continue
    }
    $index = $trimmed.IndexOf('=')
    if ($index -lt 1) {
      continue
    }
    $name = $trimmed.Substring(0, $index).Trim()
    $value = $trimmed.Substring($index + 1).Trim()
    if (
      ($value.StartsWith('"') -and $value.EndsWith('"')) -or
      ($value.StartsWith("'") -and $value.EndsWith("'"))
    ) {
      $value = $value.Substring(1, $value.Length - 2)
    }
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
      [Environment]::SetEnvironmentVariable($name, $value)
    }
  }
}

function Get-RepoHash {
  param([string]$Value)

  $sha1 = [System.Security.Cryptography.SHA1]::Create()
  try {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Value)
    $hash = $sha1.ComputeHash($bytes)
    return -join ($hash | ForEach-Object { $_.ToString('x2') }) | ForEach-Object { $_.Substring(0, 8) }
  } finally {
    $sha1.Dispose()
  }
}

function Convert-WindowsPathToWslPath {
  param([string]$Path)

  $fullPath = [System.IO.Path]::GetFullPath($Path)
  if ($fullPath -notmatch '^[A-Za-z]:\\') {
    throw "Unsupported Windows path for WSL conversion: $fullPath"
  }

  $drive = $fullPath.Substring(0, 1).ToLowerInvariant()
  $rest = $fullPath.Substring(2).Replace('\', '/')
  return "/mnt/$drive$rest"
}

function Quote-Bash {
  param([string]$Value)

  return "'" + $Value.Replace("'", "'""'""'") + "'"
}

function Get-InstalledWslDistros {
  $output = & wsl.exe -l -q 2>$null
  if ($LASTEXITCODE -ne 0) {
    return @()
  }
  return @(
    $output |
      ForEach-Object { $_ -replace "`0", '' } |
      ForEach-Object { $_.Trim() } |
      Where-Object { $_ }
  )
}

function Resolve-WslDistro {
  param([string[]]$InstalledDistros)

  $requested = $Distro
  if ([string]::IsNullOrWhiteSpace($requested)) {
    $requested = $env:WSL_DEV_DISTRO
  }

  if ($requested) {
    if ($InstalledDistros -contains $requested) {
      return $requested
    }
    throw "Configured WSL distro '$requested' is not installed. Installed distros: $($InstalledDistros -join ', ')"
  }

  if ($InstalledDistros.Count -gt 0) {
    return $InstalledDistros[0]
  }

  $message = @(
    'No WSL distro is installed on this machine.',
    '',
    'Recommended next steps:',
    '1. Run `wsl.exe --install -d Ubuntu`',
    '2. Reboot if Windows asks for it',
    '3. Open Ubuntu once to finish first-run setup',
    '4. Run `make bootstrap-wsl`'
  ) -join "`n"
  throw $message
}

function Invoke-WslBash {
  param(
    [Parameter(Mandatory = $true)][string]$ResolvedDistro,
    [Parameter(Mandatory = $true)][string]$Command
  )

  if ($env:CHATOS_WSL_DEBUG -eq '1') {
    Write-Host "WSL distro: $ResolvedDistro"
    Write-Host "WSL command: $Command"
  }

  & wsl.exe -d $ResolvedDistro -- bash -lc $Command
  exit $LASTEXITCODE
}

try {
  $scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
  $repoRoot = Split-Path -Parent $scriptDir

  Import-DotEnvFile -Path (Join-Path $repoRoot '.env')

  $installedDistros = Get-InstalledWslDistros
  $resolvedDistro = Resolve-WslDistro -InstalledDistros $installedDistros
  $repoHash = Get-RepoHash -Value $repoRoot
  $repoRootWsl = Convert-WindowsPathToWslPath -Path $repoRoot

  $defaultCargoTargetDirExpr = '"$HOME/.cache/chatos_rs/' + $repoHash + '/cargo-target"'
  $cargoTargetExport = if ($env:WSL_CARGO_TARGET_DIR) {
    'export CARGO_TARGET_DIR=' + (Quote-Bash -Value $env:WSL_CARGO_TARGET_DIR)
  } else {
    'export CARGO_TARGET_DIR=' + $defaultCargoTargetDirExpr
  }

  $wslRuntimeDir = if ($env:WSL_RUNTIME_DIR) {
    $env:WSL_RUNTIME_DIR
  } else {
    "/tmp/chatos_rs_dev_${repoHash}_wsl"
  }

  $wslUserServiceRuntimeDir = if ($env:WSL_USER_SERVICE_RUNTIME_DIR) {
    $env:WSL_USER_SERVICE_RUNTIME_DIR
  } else {
    "/tmp/chatos_rs_user_service_${repoHash}_wsl"
  }

  $wslTaskRunnerRuntimeDir = if ($env:WSL_TASK_RUNNER_RUNTIME_DIR) {
    $env:WSL_TASK_RUNNER_RUNTIME_DIR
  } else {
    "/tmp/chatos_rs_task_runner_${repoHash}_wsl"
  }

  $wslMemoryEngineRuntimeDir = if ($env:WSL_MEMORY_ENGINE_RUNTIME_DIR) {
    $env:WSL_MEMORY_ENGINE_RUNTIME_DIR
  } else {
    "/tmp/chatos_rs_memory_engine_${repoHash}_wsl"
  }

  if ($Action -eq 'bootstrap') {
    $bootstrapCommand =
      'cd ' + (Quote-Bash -Value $repoRootWsl) +
      ' && bash ./scripts/bootstrap-wsl-dev.sh'
    Invoke-WslBash -ResolvedDistro $resolvedDistro -Command $bootstrapCommand
  }

  $serviceScript = switch ($Target) {
    'main' { './restart_services.sh' }
    'user-service' { './user_service/restart_services.sh' }
    'task-runner' { './restart_task_runner_service.sh' }
    'memory-engine' { './memory_engine/restart_services.sh' }
    'all' { './restart_all_services.sh' }
  }

  $command =
    'cd ' + (Quote-Bash -Value $repoRootWsl) +
    ' && ' + $cargoTargetExport

  if ($Target -in @('main', 'all')) {
    $command += ' && export RUNTIME_DIR=' + (Quote-Bash -Value $wslRuntimeDir)
  }

  if ($Target -in @('user-service', 'all')) {
    $command += ' && export USER_SERVICE_RUNTIME_DIR=' + (Quote-Bash -Value $wslUserServiceRuntimeDir)
  }
  if ($Target -in @('task-runner', 'all')) {
    $command += ' && export TASK_RUNNER_RUNTIME_DIR=' + (Quote-Bash -Value $wslTaskRunnerRuntimeDir)
  }
  if ($Target -in @('memory-engine', 'all')) {
    $command += ' && export MEMORY_ENGINE_RUNTIME_DIR=' + (Quote-Bash -Value $wslMemoryEngineRuntimeDir)
  }
  if ($Target -eq 'task-runner') {
    $command += ' && CHATOS_RS_SHELL_SANITIZED=1 CHATOS_RS_SCRIPT_PATH=' + (Quote-Bash -Value $serviceScript) + ' bash <(tr -d ''\r'' < ' + (Quote-Bash -Value $serviceScript) + ') ' + (Quote-Bash -Value $Action)
  } else {
    $command += ' && ' + (Quote-Bash -Value $serviceScript) + ' ' + (Quote-Bash -Value $Action)
  }
  Invoke-WslBash -ResolvedDistro $resolvedDistro -Command $command
} catch {
  [Console]::Error.WriteLine($_.Exception.Message)
  exit 1
}
