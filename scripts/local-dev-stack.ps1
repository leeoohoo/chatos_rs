[CmdletBinding()]
param(
  [ValidateSet('restart', 'start', 'stop', 'status')]
  [string]$Action = 'status'
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Set-StrictMode -Version Latest

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

function Get-EnvOrDefault {
  param(
    [string]$Name,
    [string]$DefaultValue
  )

  $value = [Environment]::GetEnvironmentVariable($Name)
  if ([string]::IsNullOrWhiteSpace($value)) {
    return $DefaultValue
  }
  return $value.Trim()
}

function New-MongoConnectionString {
  param(
    [string]$MongoHostName,
    [string]$Port,
    [string]$Database,
    [string]$Username,
    [string]$Password,
    [string]$AuthSource
  )

  $credentials = ''
  if (-not [string]::IsNullOrWhiteSpace($Username)) {
    $encodedUser = [System.Uri]::EscapeDataString($Username)
    $encodedPassword = [System.Uri]::EscapeDataString($Password)
    $credentials = "${encodedUser}:${encodedPassword}@"
  }

  $authQuery = ''
  if (-not [string]::IsNullOrWhiteSpace($AuthSource)) {
    $authQuery = '?authSource=' + [System.Uri]::EscapeDataString($AuthSource)
  }

  return ('mongodb://{0}{1}:{2}/{3}{4}' -f $credentials, $MongoHostName, $Port, $Database, $authQuery)
}

function ConvertTo-PowerShellLiteral {
  param([string]$Value)

  return "'" + $Value.Replace("'", "''") + "'"
}

function Quote-Bash {
  param([string]$Value)

  return "'" + $Value.Replace("'", "'""'""'") + "'"
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

  $requested = [Environment]::GetEnvironmentVariable('WSL_DEV_DISTRO')
  if (-not [string]::IsNullOrWhiteSpace($requested)) {
    if ($InstalledDistros -contains $requested) {
      return $requested
    }
    throw "Configured WSL distro '$requested' is not installed. Installed distros: $($InstalledDistros -join ', ')"
  }

  $preferred = @($InstalledDistros | Where-Object { $_ -and $_ -notlike 'docker-desktop*' })
  if ($preferred.Count -gt 0) {
    return $preferred[0]
  }
  if ($InstalledDistros.Count -gt 0) {
    return $InstalledDistros[0]
  }

  throw 'No WSL distro is installed. Install Ubuntu with `wsl.exe --install -d Ubuntu` first.'
}

function New-PowerShellCommand {
  param(
    [hashtable]$Environment,
    [string]$Command
  )

  $parts = @()
  foreach ($entry in $Environment.GetEnumerator() | Sort-Object Name) {
    $parts += ('$env:{0} = {1}' -f $entry.Key, (ConvertTo-PowerShellLiteral -Value ([string]$entry.Value)))
  }
  $parts += '$ErrorActionPreference = ''Stop'''
  $parts += $Command
  return ($parts -join '; ')
}

function Stop-ProcessesOnPort {
  param([int]$Port)

  $connections = @(Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue)
  if ($connections.Count -eq 0) {
    return
  }

  $pids = @($connections | Select-Object -ExpandProperty OwningProcess -Unique)
  foreach ($pidValue in $pids) {
    if (-not $pidValue) {
      continue
    }

    try {
      Stop-Process -Id $pidValue -Force -ErrorAction Stop
    } catch {
      Write-Warning "Stop process on port $Port failed for pid=${pidValue}: $($_.Exception.Message)"
    }
  }
}

function Test-PortListening {
  param([int]$Port)

  return $null -ne (Get-NetTCPConnection -LocalPort $Port -State Listen -ErrorAction SilentlyContinue | Select-Object -First 1)
}

function Wait-PortClosed {
  param(
    [int]$Port,
    [int]$TimeoutSeconds = 20
  )

  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $deadline) {
    if (-not (Test-PortListening -Port $Port)) {
      return
    }
    Start-Sleep -Milliseconds 500
  }

  throw "Port did not close in time: $Port"
}

function Wait-HttpReady {
  param(
    [string]$Name,
    [string]$Url,
    [int]$TimeoutSeconds = 180
  )

  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $deadline) {
    try {
      $response = Invoke-WebRequest -UseBasicParsing -Uri $Url -TimeoutSec 5
      if ($response.StatusCode -ge 200 -and $response.StatusCode -lt 400) {
        return $response
      }
    } catch {
      Start-Sleep -Seconds 2
      continue
    }
    Start-Sleep -Seconds 2
  }

  throw "$Name did not become ready in time: $Url"
}

function Start-LoggedProcess {
  param(
    [string]$Name,
    [string]$WorkingDirectory,
    [string]$Command,
    [hashtable]$Environment,
    [string]$StdOutPath,
    [string]$StdErrPath
  )

  $commandText = New-PowerShellCommand -Environment $Environment -Command $Command
  if (Test-Path -LiteralPath $StdOutPath) {
    Remove-Item -LiteralPath $StdOutPath -Force
  }
  if (Test-Path -LiteralPath $StdErrPath) {
    Remove-Item -LiteralPath $StdErrPath -Force
  }

  $process = Start-Process powershell `
    -ArgumentList @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command', $commandText) `
    -WorkingDirectory $WorkingDirectory `
    -RedirectStandardOutput $StdOutPath `
    -RedirectStandardError $StdErrPath `
    -WindowStyle Hidden `
    -PassThru

  return $process
}

function Ensure-NpmInstalled {
  param(
    [string]$ProjectDir,
    [string]$Name
  )

  $nodeModulesDir = Join-Path $ProjectDir 'node_modules'
  $vitePackageJson = Join-Path $ProjectDir 'node_modules\vite\package.json'
  if ((Test-Path -LiteralPath $nodeModulesDir) -and (Test-Path -LiteralPath $vitePackageJson)) {
    return
  }

  Write-Host "[INFO] installing npm dependencies for $Name"
  & npm --prefix $ProjectDir install
  if ($LASTEXITCODE -ne 0) {
    throw "npm install failed for $Name"
  }
}

function Ensure-NativePackageExpanded {
  param(
    [string]$PackageName,
    [string]$Version,
    [string]$Destination,
    [string]$CacheRoot
  )

  $packageJsonPath = Join-Path $Destination 'package.json'
  if (Test-Path -LiteralPath $packageJsonPath) {
    return
  }

  $packageStem = $PackageName.Split('/')[-1]
  $archiveStem = $PackageName.TrimStart('@').Replace('/', '-')
  $archivePath = Join-Path $CacheRoot "$archiveStem-$Version.tgz"
  $extractDir = Join-Path $CacheRoot "extract-$archiveStem-$Version"
  $url = "https://registry.npmjs.org/$PackageName/-/$packageStem-$Version.tgz"

  New-Item -ItemType Directory -Force -Path $CacheRoot | Out-Null
  if (-not (Test-Path -LiteralPath $archivePath)) {
    Write-Host "[INFO] downloading $PackageName@$Version"
    Invoke-WebRequest -Uri $url -OutFile $archivePath
  }

  if (Test-Path -LiteralPath $extractDir) {
    Remove-Item -LiteralPath $extractDir -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $extractDir | Out-Null

  & tar -xf $archivePath -C $extractDir
  if ($LASTEXITCODE -ne 0) {
    throw "tar extraction failed for $PackageName@$Version"
  }

  if (Test-Path -LiteralPath $Destination) {
    Remove-Item -LiteralPath $Destination -Recurse -Force
  }
  New-Item -ItemType Directory -Force -Path $Destination | Out-Null
  Get-ChildItem -LiteralPath (Join-Path $extractDir 'package') -Force |
    ForEach-Object {
      Copy-Item -LiteralPath $_.FullName -Destination $Destination -Recurse -Force
    }
}

function Ensure-FrontendNativePackages {
  param(
    [string]$ProjectDir,
    [string]$Name,
    [string]$CacheRoot
  )

  $checks = @(
    @{
      PackageJson = Join-Path $ProjectDir 'node_modules\rollup\package.json'
      PackageName = '@rollup/rollup-win32-x64-msvc'
      Destination = Join-Path $ProjectDir 'node_modules\@rollup\rollup-win32-x64-msvc'
    },
    @{
      PackageJson = Join-Path $ProjectDir 'node_modules\esbuild\package.json'
      PackageName = '@esbuild/win32-x64'
      Destination = Join-Path $ProjectDir 'node_modules\@esbuild\win32-x64'
    },
    @{
      PackageJson = Join-Path $ProjectDir 'node_modules\vite\node_modules\esbuild\package.json'
      PackageName = '@esbuild/win32-x64'
      Destination = Join-Path $ProjectDir 'node_modules\vite\node_modules\@esbuild\win32-x64'
    }
  )

  foreach ($check in $checks) {
    if (-not (Test-Path -LiteralPath $check.PackageJson)) {
      continue
    }

    $pkg = Get-Content -LiteralPath $check.PackageJson -Raw | ConvertFrom-Json
    Ensure-NativePackageExpanded `
      -PackageName $check.PackageName `
      -Version $pkg.version `
      -Destination $check.Destination `
      -CacheRoot $CacheRoot
  }

  Write-Host "[INFO] native node packages ready for $Name"
}

function Ensure-FrontendDependencies {
  param(
    [string]$ProjectDir,
    [string]$Name,
    [string]$CacheRoot
  )

  Ensure-NpmInstalled -ProjectDir $ProjectDir -Name $Name
  Ensure-FrontendNativePackages -ProjectDir $ProjectDir -Name $Name -CacheRoot $CacheRoot
}

function Build-ChatAppBackend {
  param([string]$RepoRoot)

  Write-Host '[INFO] building chat_app backend'
  & cargo build --manifest-path (Join-Path $RepoRoot 'chat_app_server_rs\Cargo.toml') --target-dir (Join-Path $RepoRoot 'target-shared')
  if ($LASTEXITCODE -ne 0) {
    throw 'chat_app backend build failed'
  }
}

function Build-TaskRunnerBackend {
  param([string]$RepoRoot)

  Write-Host '[INFO] building task_runner backend'
  & cargo build --manifest-path (Join-Path $RepoRoot 'task_runner_service\backend\Cargo.toml') --bin task_runner_service_backend --target-dir (Join-Path $RepoRoot 'target-shared')
  if ($LASTEXITCODE -ne 0) {
    throw 'task_runner backend build failed'
  }
}

function Invoke-WslMongoScript {
  param(
    [string]$ResolvedDistro,
    [string]$RepoRoot,
    [ValidateSet('restart', 'start', 'stop', 'status')]
    [string]$MongoAction
  )

  $repoRootWsl = Convert-WindowsPathToWslPath -Path $RepoRoot
  $mongoScript = "$repoRootWsl/scripts/restart_local_mongo.sh"
  $bootstrapScript = "$repoRootWsl/scripts/bootstrap_local_mongo_admin.py"
  $tmpScript = "/tmp/chatos_restart_local_mongo_$PID.sh"

  $command =
    'tr -d ''\r'' < ' + (Quote-Bash -Value $mongoScript) +
    ' > ' + (Quote-Bash -Value $tmpScript) +
    ' && LOCAL_MONGO_BOOTSTRAP_SCRIPT=' + (Quote-Bash -Value $bootstrapScript) +
    ' bash ' + (Quote-Bash -Value $tmpScript) +
    ' ' + (Quote-Bash -Value $MongoAction)

  & wsl.exe -d $ResolvedDistro -- bash -lc $command
  if ($LASTEXITCODE -ne 0) {
    throw "WSL local Mongo action failed: $MongoAction"
  }
}

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$repoRoot = Split-Path -Parent $scriptDir
$runDir = Join-Path $repoRoot '.local\run'
$nativeCacheDir = Join-Path $runDir 'npm-native-cache'
. (Join-Path $scriptDir 'local-dev-stack\config.ps1')

function Show-StackStatus {
  Write-Host 'Windows local stack status:'
  foreach ($service in $serviceDefinitions) {
    $listening = Test-PortListening -Port $service.Port
    Write-Host ("  {0,-28} {1,-9} {2}" -f $service.Name, ($(if ($listening) { 'up' } else { 'down' })), $service.Url)
  }
  foreach ($frontend in $frontendDefinitions) {
    $listening = Test-PortListening -Port $frontend.Port
    Write-Host ("  {0,-28} {1,-9} {2}" -f $frontend.Name, ($(if ($listening) { 'up' } else { 'down' })), $frontend.Url)
  }

  $mongoListening = Test-PortListening -Port 27018
  Write-Host ("  {0,-28} {1,-9} {2}" -f 'wsl_local_mongo', ($(if ($mongoListening) { 'up' } else { 'down' })), 'mongodb://127.0.0.1:27018')
  Write-Host ''
  Write-Host "Logs: $runDir"
}

function Start-Stack {
  New-Item -ItemType Directory -Force -Path $runDir | Out-Null
  New-Item -ItemType Directory -Force -Path $nativeCacheDir | Out-Null

  Get-ChildItem -LiteralPath $runDir -Filter 'task_runner.dev.db*' -ErrorAction SilentlyContinue |
    Remove-Item -Force -ErrorAction SilentlyContinue

  Build-ChatAppBackend -RepoRoot $repoRoot
  Build-TaskRunnerBackend -RepoRoot $repoRoot

  $chatBackendExe = Join-Path $repoRoot 'target-shared\debug\chat_app_server_rs.exe'
  if (-not (Test-Path -LiteralPath $chatBackendExe)) {
    throw "Missing chat_app backend executable: $chatBackendExe"
  }

  $taskRunnerBackendExe = Join-Path $repoRoot 'target-shared\debug\task_runner_service_backend.exe'
  if (-not (Test-Path -LiteralPath $taskRunnerBackendExe)) {
    throw "Missing task_runner backend executable: $taskRunnerBackendExe"
  }

  $installedDistros = Get-InstalledWslDistros
  $wslDistro = Resolve-WslDistro -InstalledDistros $installedDistros

  Write-Host "[INFO] starting local Mongo in WSL distro $wslDistro"
  Invoke-WslMongoScript -ResolvedDistro $wslDistro -RepoRoot $repoRoot -MongoAction restart

  foreach ($service in $serviceDefinitions) {
    Write-Host "[INFO] starting $($service.Name)"
    [void](Start-LoggedProcess `
      -Name $service.Name `
      -WorkingDirectory $service.WorkingDirectory `
      -Command $service.Command `
      -Environment $service.Environment `
      -StdOutPath $service.StdOut `
      -StdErrPath $service.StdErr)
    [void](Wait-HttpReady -Name $service.Name -Url $service.Url)
  }

  foreach ($frontend in $frontendDefinitions) {
    Ensure-FrontendDependencies -ProjectDir $frontend.ProjectDir -Name $frontend.Name -CacheRoot $nativeCacheDir
    Write-Host "[INFO] starting $($frontend.Name)"
    [void](Start-LoggedProcess `
      -Name $frontend.Name `
      -WorkingDirectory $repoRoot `
      -Command $frontend.Command `
      -Environment @{} `
      -StdOutPath $frontend.StdOut `
      -StdErrPath $frontend.StdErr)
    [void](Wait-HttpReady -Name $frontend.Name -Url $frontend.Url -TimeoutSeconds 120)
  }

  Write-Host ''
  Write-Host '[OK] Windows local stack is ready'
  Write-Host '  chat_app:                 http://127.0.0.1:8088'
  Write-Host '  task_runner frontend:     http://127.0.0.1:39091'
  Write-Host '  project_management:       http://127.0.0.1:39211'
  Write-Host '  user_service frontend:    http://127.0.0.1:39191'
  Write-Host '  memory_engine frontend:   http://127.0.0.1:4178'
  Write-Host '  admin username:           admin'
  Write-Host '  admin password:           admin123456'
  Write-Host ''
  Write-Host "Logs: $runDir"
}

function Stop-Stack {
  foreach ($port in ($ports | Sort-Object -Descending)) {
    Stop-ProcessesOnPort -Port $port
  }

  foreach ($port in $ports) {
    try {
      Wait-PortClosed -Port $port -TimeoutSeconds 10
    } catch {
      Write-Warning $_.Exception.Message
    }
  }

  $installedDistros = Get-InstalledWslDistros
  if ($installedDistros.Count -gt 0) {
    try {
      $wslDistro = Resolve-WslDistro -InstalledDistros $installedDistros
      Invoke-WslMongoScript -ResolvedDistro $wslDistro -RepoRoot $repoRoot -MongoAction stop
    } catch {
      Write-Warning "WSL local Mongo stop skipped: $($_.Exception.Message)"
    }
  }

  Write-Host '[OK] Windows local stack stopped'
}

switch ($Action) {
  'restart' {
    Stop-Stack
    Start-Stack
  }
  'start' {
    Start-Stack
  }
  'stop' {
    Stop-Stack
  }
  'status' {
    Show-StackStatus
  }
  default {
    throw "Unsupported action: $Action"
  }
}
