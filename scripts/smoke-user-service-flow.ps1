[CmdletBinding()]
param(
  [string]$BaseUrl = "http://127.0.0.1:39190",
  [string]$UserPassword = "Pass123456",
  [string]$AgentPassword = "Agent123456",
  [string]$ContactId = "smoke-contact"
)

$ErrorActionPreference = 'Stop'

function Normalize-BaseUrl {
  param([string]$Value)
  return $Value.TrimEnd('/')
}

function Invoke-JsonRequest {
  param(
    [Parameter(Mandatory = $true)][string]$Method,
    [Parameter(Mandatory = $true)][string]$Path,
    [object]$Body,
    [string]$Token
  )

  $headers = @{}
  if ($Token) {
    $headers['Authorization'] = "Bearer $Token"
  }

  $request = @{
    Method      = $Method
    Uri         = "$script:NormalizedBaseUrl$Path"
    Headers     = $headers
    ContentType = 'application/json'
  }

  if ($null -ne $Body) {
    $request['Body'] = ($Body | ConvertTo-Json -Depth 8)
  }

  return Invoke-RestMethod @request
}

function ConvertFrom-Base64Url {
  param([Parameter(Mandatory = $true)][string]$Value)

  $normalized = $Value.Replace('-', '+').Replace('_', '/')
  switch ($normalized.Length % 4) {
    2 { $normalized += '==' }
    3 { $normalized += '=' }
    0 { }
    default { throw "invalid base64url payload length" }
  }

  $bytes = [Convert]::FromBase64String($normalized)
  return [System.Text.Encoding]::UTF8.GetString($bytes)
}

function Decode-JwtPayload {
  param([Parameter(Mandatory = $true)][string]$Token)

  $parts = $Token.Split('.')
  if ($parts.Length -lt 2) {
    throw "invalid JWT format"
  }

  $payloadJson = ConvertFrom-Base64Url -Value $parts[1]
  return $payloadJson | ConvertFrom-Json
}

$script:NormalizedBaseUrl = Normalize-BaseUrl -Value $BaseUrl
$health = Invoke-RestMethod -Uri "$script:NormalizedBaseUrl/api/health"
if ($health.status -ne 'ok') {
  throw "health check failed"
}

$suffix = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds()
$username = "smoke_user_$suffix"
$agentUsername = "smoke_agent_$suffix"

$register = Invoke-JsonRequest -Method 'POST' -Path '/api/auth/register' -Body @{
  username = $username
  password = $UserPassword
}
if (-not $register.token) {
  throw "register did not return a token"
}

$userToken = [string]$register.token
$me = Invoke-JsonRequest -Method 'GET' -Path '/api/auth/me' -Token $userToken
if (-not $me.user.id) {
  throw "current user lookup did not return an id"
}

$agent = Invoke-JsonRequest -Method 'POST' -Path '/api/agent-accounts' -Token $userToken -Body @{
  username     = $agentUsername
  display_name = "Smoke Agent $suffix"
  password     = $AgentPassword
  enabled      = $true
}
if (-not $agent.id) {
  throw "agent creation did not return an id"
}
if ($agent.owner_user_id -ne $me.user.id) {
  throw "agent owner_user_id does not match the current user"
}

$agents = Invoke-JsonRequest -Method 'GET' -Path '/api/agent-accounts' -Token $userToken
if (-not ($agents | Where-Object { $_.id -eq $agent.id })) {
  throw "created agent was not returned by list_agent_accounts"
}

$exchange = Invoke-JsonRequest -Method 'POST' -Path '/api/token/exchange/task-runner' -Token $userToken -Body @{
  task_runner_agent_account_id = $agent.id
  contact_id = $ContactId
}
if (-not $exchange.access_token) {
  throw "token exchange did not return access_token"
}

$claims = Decode-JwtPayload -Token ([string]$exchange.access_token)
if ($claims.principal_type -ne 'agent_account') {
  throw "unexpected principal_type: $($claims.principal_type)"
}
if ($claims.agent_account_id -ne $agent.id) {
  throw "JWT agent_account_id does not match the created agent"
}
if ($claims.owner_user_id -ne $me.user.id) {
  throw "JWT owner_user_id does not match the current user"
}

[PSCustomObject]@{
  base_url = $script:NormalizedBaseUrl
  user = [PSCustomObject]@{
    id = $me.user.id
    username = $username
  }
  agent = [PSCustomObject]@{
    id = $agent.id
    username = $agent.username
    owner_user_id = $agent.owner_user_id
  }
  exchanged_principal = [PSCustomObject]@{
    principal_type = $exchange.principal.principal_type
    agent_account_id = $exchange.principal.agent_account_id
    owner_user_id = $exchange.principal.owner_user_id
  }
  jwt_claims = [PSCustomObject]@{
    aud = $claims.aud
    principal_type = $claims.principal_type
    agent_account_id = $claims.agent_account_id
    owner_user_id = $claims.owner_user_id
  }
} | ConvertTo-Json -Depth 8
