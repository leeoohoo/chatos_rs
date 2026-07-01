# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

Import-DotEnvFile -Path (Join-Path $repoRoot '.env')

$projectSyncSecret = Get-EnvOrDefault -Name 'PROJECT_SERVICE_SYNC_SECRET' -DefaultValue (Get-EnvOrDefault -Name 'CHATOS_PROJECT_SERVICE_SYNC_SECRET' -DefaultValue 'change_me_project_sync_secret')
$taskRunnerCallbackSecret = Get-EnvOrDefault -Name 'TASK_RUNNER_CHATOS_CALLBACK_SECRET' -DefaultValue 'change_me_chatos_task_runner_secret'
$memoryEngineOperatorToken = Get-EnvOrDefault -Name 'MEMORY_ENGINE_OPERATOR_TOKEN' -DefaultValue 'chatos-memory-engine-dev-operator-token'
$userJwtSecret = Get-EnvOrDefault -Name 'USER_SERVICE_JWT_SECRET' -DefaultValue 'change_me_user_service_secret'
$userJwtIssuer = Get-EnvOrDefault -Name 'USER_SERVICE_JWT_ISSUER' -DefaultValue 'user_service'
$userAudience = Get-EnvOrDefault -Name 'USER_SERVICE_USER_AUDIENCE' -DefaultValue 'user_service'
$taskRunnerAudience = Get-EnvOrDefault -Name 'USER_SERVICE_TASK_RUNNER_AUDIENCE' -DefaultValue 'task_runner'
$authJwtSecret = Get-EnvOrDefault -Name 'AUTH_JWT_SECRET' -DefaultValue 'dev-only-change-me-please'
$mongoHost = Get-EnvOrDefault -Name 'MONGODB_HOST' -DefaultValue '127.0.0.1'
$mongoPort = Get-EnvOrDefault -Name 'MONGODB_PORT' -DefaultValue '27018'
$mongoUser = Get-EnvOrDefault -Name 'MONGODB_USER' -DefaultValue 'admin'
$mongoPassword = Get-EnvOrDefault -Name 'MONGODB_PASSWORD' -DefaultValue 'admin'
$mongoAuthSource = Get-EnvOrDefault -Name 'MONGODB_AUTH_SOURCE' -DefaultValue 'admin'
$chatAppMongoDatabase = Get-EnvOrDefault -Name 'MONGODB_DB' -DefaultValue 'chatos'
$projectServiceMongoDatabase = Get-EnvOrDefault -Name 'PROJECT_SERVICE_MONGODB_DATABASE' -DefaultValue 'project_management_service'
$memoryEngineMongoDatabase = Get-EnvOrDefault -Name 'MEMORY_ENGINE_MONGODB_DATABASE' -DefaultValue 'memory_engine'
$taskRunnerMongoDatabase = Get-EnvOrDefault -Name 'TASK_RUNNER_MONGODB_DATABASE' -DefaultValue 'task_runner_service'
$userServiceMongoDatabase = Get-EnvOrDefault -Name 'USER_SERVICE_MONGODB_DATABASE' -DefaultValue 'user_service'
$chatAppDatabaseType = Get-EnvOrDefault -Name 'DATABASE_TYPE' -DefaultValue 'mongodb'
$userServiceDatabaseUrl = Get-EnvOrDefault -Name 'USER_SERVICE_DATABASE_URL' -DefaultValue (New-MongoConnectionString -MongoHostName $mongoHost -Port $mongoPort -Database $userServiceMongoDatabase -Username $mongoUser -Password $mongoPassword -AuthSource $mongoAuthSource)
$projectServiceDatabaseUrl = Get-EnvOrDefault -Name 'PROJECT_SERVICE_DATABASE_URL' -DefaultValue (New-MongoConnectionString -MongoHostName $mongoHost -Port $mongoPort -Database $projectServiceMongoDatabase -Username $mongoUser -Password $mongoPassword -AuthSource $mongoAuthSource)
$memoryEngineMongoUri = Get-EnvOrDefault -Name 'MEMORY_ENGINE_MONGODB_URI' -DefaultValue (New-MongoConnectionString -MongoHostName $mongoHost -Port $mongoPort -Database 'admin' -Username $mongoUser -Password $mongoPassword -AuthSource $mongoAuthSource)
$taskRunnerDatabaseUrl = Get-EnvOrDefault -Name 'TASK_RUNNER_DATABASE_URL' -DefaultValue (New-MongoConnectionString -MongoHostName $mongoHost -Port $mongoPort -Database $taskRunnerMongoDatabase -Username $mongoUser -Password $mongoPassword -AuthSource $mongoAuthSource)

$chatAppBackendEnvironment = @{
  NODE_ENV = 'development'
  HOST = '127.0.0.1'
  BACKEND_PORT = '3997'
  DATABASE_TYPE = $chatAppDatabaseType
  AUTH_JWT_SECRET = $authJwtSecret
  CHATOS_USER_SERVICE_BASE_URL = 'http://127.0.0.1:39190'
  CHATOS_USER_SERVICE_REQUEST_TIMEOUT_MS = '5000'
  CHATOS_USER_SERVICE_JWT_SECRET = $userJwtSecret
  CHATOS_USER_SERVICE_JWT_ISSUER = $userJwtIssuer
  CHATOS_USER_SERVICE_USER_AUDIENCE = $userAudience
  CHATOS_PROJECT_SERVICE_BASE_URL = 'http://127.0.0.1:39210'
  CHATOS_PROJECT_SERVICE_SYNC_SECRET = $projectSyncSecret
  CHATOS_TASK_RUNNER_BASE_URL = 'http://127.0.0.1:39090'
  CHATOS_TASK_RUNNER_REQUEST_TIMEOUT_MS = '30000'
  TASK_RUNNER_CHATOS_CALLBACK_SECRET = $taskRunnerCallbackSecret
  MEMORY_ENGINE_BASE_URL = 'http://127.0.0.1:7081/api/memory-engine/v1'
  MEMORY_ENGINE_OPERATOR_TOKEN = $memoryEngineOperatorToken
  MEMORY_ENGINE_REQUEST_TIMEOUT_MS = '5000'
}

if ($chatAppDatabaseType.Trim().ToLowerInvariant() -eq 'mongodb') {
  $chatAppBackendEnvironment['MONGODB_HOST'] = $mongoHost
  $chatAppBackendEnvironment['MONGODB_PORT'] = $mongoPort
  $chatAppBackendEnvironment['MONGODB_DB'] = $chatAppMongoDatabase
  $chatAppBackendEnvironment['MONGODB_USER'] = $mongoUser
  $chatAppBackendEnvironment['MONGODB_PASSWORD'] = $mongoPassword
  $chatAppBackendEnvironment['MONGODB_AUTH_SOURCE'] = $mongoAuthSource

  $mongoConnectionString = [Environment]::GetEnvironmentVariable('MONGODB_CONNECTION_STRING')
  if (-not [string]::IsNullOrWhiteSpace($mongoConnectionString)) {
    $chatAppBackendEnvironment['MONGODB_CONNECTION_STRING'] = $mongoConnectionString.Trim()
  }
} else {
  foreach ($key in @(
    'MONGODB_CONNECTION_STRING',
    'MONGODB_HOST',
    'MONGODB_PORT',
    'MONGODB_DB',
    'MONGODB_USER',
    'MONGODB_PASSWORD',
    'MONGODB_AUTH_SOURCE'
  )) {
    $chatAppBackendEnvironment[$key] = ''
  }
}

$ports = @(39190, 39210, 39090, 7081, 3997, 39191, 39211, 39091, 4178, 8088)

$serviceDefinitions = @(
  [pscustomobject]@{
    Name = 'user_service_backend'
    Port = 39190
    Url = 'http://127.0.0.1:39190/api/health'
    StdOut = Join-Path $runDir 'user_service_backend.out.log'
    StdErr = Join-Path $runDir 'user_service_backend.err.log'
    WorkingDirectory = $repoRoot
    Command = 'cargo run --manifest-path user_service/backend/Cargo.toml --bin user_service_backend'
    Environment = @{
      CARGO_TARGET_DIR = 'target-user-run'
      USER_SERVICE_HOST = '127.0.0.1'
      USER_SERVICE_PORT = '39190'
      USER_SERVICE_DATABASE_URL = $userServiceDatabaseUrl
      USER_SERVICE_JWT_SECRET = $userJwtSecret
      USER_SERVICE_JWT_ISSUER = $userJwtIssuer
      USER_SERVICE_USER_AUDIENCE = $userAudience
      USER_SERVICE_TASK_RUNNER_AUDIENCE = $taskRunnerAudience
      USER_SERVICE_SUPER_ADMIN_USERNAME = 'admin'
      USER_SERVICE_SUPER_ADMIN_PASSWORD = 'admin123456'
      USER_SERVICE_SUPER_ADMIN_DISPLAY_NAME = 'System Admin'
      TASK_RUNNER_BASE_URL = 'http://127.0.0.1:39090'
      TASK_RUNNER_CHATOS_CALLBACK_SECRET = $taskRunnerCallbackSecret
      MEMORY_ENGINE_BASE_URL = 'http://127.0.0.1:7081/api/memory-engine/v1'
      MEMORY_ENGINE_OPERATOR_TOKEN = $memoryEngineOperatorToken
    }
  },
  [pscustomobject]@{
    Name = 'project_management_backend'
    Port = 39210
    Url = 'http://127.0.0.1:39210/api/health'
    StdOut = Join-Path $runDir 'project_management_backend.out.log'
    StdErr = Join-Path $runDir 'project_management_backend.err.log'
    WorkingDirectory = $repoRoot
    Command = 'cargo run --manifest-path project_management_service/backend/Cargo.toml --bin project_management_service_backend'
    Environment = @{
      CARGO_TARGET_DIR = 'target-pm-run'
      PROJECT_SERVICE_HOST = '127.0.0.1'
      PROJECT_SERVICE_PORT = '39210'
      PROJECT_SERVICE_DATABASE_URL = $projectServiceDatabaseUrl
      PROJECT_SERVICE_USER_SERVICE_BASE_URL = 'http://127.0.0.1:39190'
      PROJECT_SERVICE_USER_SERVICE_REQUEST_TIMEOUT_MS = '5000'
      PROJECT_SERVICE_TASK_RUNNER_BASE_URL = 'http://127.0.0.1:39090'
      PROJECT_SERVICE_TASK_RUNNER_REQUEST_TIMEOUT_MS = '10000'
      PROJECT_SERVICE_SYNC_SECRET = $projectSyncSecret
    }
  },
  [pscustomobject]@{
    Name = 'memory_engine_backend'
    Port = 7081
    Url = 'http://127.0.0.1:7081/health'
    StdOut = Join-Path $runDir 'memory_engine_backend.out.log'
    StdErr = Join-Path $runDir 'memory_engine_backend.err.log'
    WorkingDirectory = $repoRoot
    Command = 'cargo run --manifest-path memory_engine/backend/Cargo.toml --bin memory_engine'
    Environment = @{
      CARGO_TARGET_DIR = 'target-me-run'
      MEMORY_ENGINE_HOST = '127.0.0.1'
      MEMORY_ENGINE_PORT = '7081'
      MEMORY_ENGINE_MONGODB_URI = $memoryEngineMongoUri
      MEMORY_ENGINE_MONGODB_DATABASE = $memoryEngineMongoDatabase
      MEMORY_ENGINE_USER_SERVICE_BASE_URL = 'http://127.0.0.1:39190'
      MEMORY_ENGINE_USER_SERVICE_REQUEST_TIMEOUT_MS = '5000'
      MEMORY_ENGINE_OPERATOR_TOKEN = $memoryEngineOperatorToken
    }
  },
  [pscustomobject]@{
    Name = 'task_runner_backend'
    Port = 39090
    Url = 'http://127.0.0.1:39090/api/health'
    StdOut = Join-Path $runDir 'task_runner_backend.out.log'
    StdErr = Join-Path $runDir 'task_runner_backend.err.log'
    WorkingDirectory = $repoRoot
    Command = '& ".\target-shared\debug\task_runner_service_backend.exe"'
    Environment = @{
      TASK_RUNNER_HOST = '127.0.0.1'
      TASK_RUNNER_PORT = '39090'
      TASK_RUNNER_STORE_MODE = 'mongo'
      TASK_RUNNER_DATABASE_URL = $taskRunnerDatabaseUrl
      TASK_RUNNER_WORKSPACE_DIR = $repoRoot
      TASK_RUNNER_ADMIN_USERNAME = 'admin'
      TASK_RUNNER_ADMIN_PASSWORD = 'admin123456'
      TASK_RUNNER_ADMIN_DISPLAY_NAME = 'System Admin'
      TASK_RUNNER_USER_SERVICE_BASE_URL = 'http://127.0.0.1:39190'
      TASK_RUNNER_USER_SERVICE_REQUEST_TIMEOUT_MS = '5000'
      TASK_RUNNER_PROJECT_SERVICE_BASE_URL = 'http://127.0.0.1:39210'
      TASK_RUNNER_PROJECT_SERVICE_SYNC_SECRET = $projectSyncSecret
      TASK_RUNNER_PROJECT_SERVICE_REQUEST_TIMEOUT_MS = '5000'
      TASK_RUNNER_CHATOS_CALLBACK_URL = 'http://127.0.0.1:3997/api/agent/chat/task-runner/callback'
      TASK_RUNNER_CHATOS_CALLBACK_SECRET = $taskRunnerCallbackSecret
      TASK_RUNNER_MEMORY_ENGINE_BASE_URL = 'http://127.0.0.1:7081/api/memory-engine/v1'
      TASK_RUNNER_MEMORY_ENGINE_OPERATOR_TOKEN = $memoryEngineOperatorToken
      TASK_RUNNER_MEMORY_ENGINE_SOURCE_ID = 'task'
    }
  },
  [pscustomobject]@{
    Name = 'chat_app_backend'
    Port = 3997
    Url = 'http://127.0.0.1:3997/health'
    StdOut = Join-Path $runDir 'chat_app_backend.out.log'
    StdErr = Join-Path $runDir 'chat_app_backend.err.log'
    WorkingDirectory = $repoRoot
    Command = '& ".\target-shared\debug\chat_app_server_rs.exe"'
    Environment = $chatAppBackendEnvironment
  }
)

$frontendDefinitions = @(
  [pscustomobject]@{
    Name = 'user_service_frontend'
    Port = 39191
    Url = 'http://127.0.0.1:39191'
    ProjectDir = Join-Path $repoRoot 'user_service\frontend'
    StdOut = Join-Path $runDir 'user_service_frontend.out.log'
    StdErr = Join-Path $runDir 'user_service_frontend.err.log'
    Command = 'npm --prefix user_service/frontend run dev -- --host 127.0.0.1'
  },
  [pscustomobject]@{
    Name = 'project_management_frontend'
    Port = 39211
    Url = 'http://127.0.0.1:39211'
    ProjectDir = Join-Path $repoRoot 'project_management_service\frontend'
    StdOut = Join-Path $runDir 'project_management_frontend.out.log'
    StdErr = Join-Path $runDir 'project_management_frontend.err.log'
    Command = 'npm --prefix project_management_service/frontend run dev -- --host 127.0.0.1'
  },
  [pscustomobject]@{
    Name = 'task_runner_frontend'
    Port = 39091
    Url = 'http://127.0.0.1:39091'
    ProjectDir = Join-Path $repoRoot 'task_runner_service\frontend'
    StdOut = Join-Path $runDir 'task_runner_frontend.out.log'
    StdErr = Join-Path $runDir 'task_runner_frontend.err.log'
    Command = 'npm --prefix task_runner_service/frontend run dev -- --host 127.0.0.1'
  },
  [pscustomobject]@{
    Name = 'memory_engine_frontend'
    Port = 4178
    Url = 'http://127.0.0.1:4178'
    ProjectDir = Join-Path $repoRoot 'memory_engine\frontend'
    StdOut = Join-Path $runDir 'memory_engine_frontend.out.log'
    StdErr = Join-Path $runDir 'memory_engine_frontend.err.log'
    Command = 'npm --prefix memory_engine/frontend run dev -- --host 127.0.0.1'
  },
  [pscustomobject]@{
    Name = 'chat_app_frontend'
    Port = 8088
    Url = 'http://127.0.0.1:8088'
    ProjectDir = Join-Path $repoRoot 'chat_app'
    StdOut = Join-Path $runDir 'chat_app_frontend.out.log'
    StdErr = Join-Path $runDir 'chat_app_frontend.err.log'
    Command = 'npm --prefix chat_app run dev -- --host 127.0.0.1'
  }
)
