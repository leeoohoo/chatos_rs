#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASE_URL="${MCP_MANAGEMENT_SMOKE_BASE_URL:-http://127.0.0.1:${MCP_MANAGEMENT_PORT:-39280}}"
BASE_URL="${BASE_URL%/}"

require_command() {
  local command_name="$1"
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "[ERROR] $command_name is required" >&2
    exit 1
  fi
}

require_command curl
require_command node

if ! curl -fsS --max-time 5 "$BASE_URL/health" >/dev/null; then
  echo "[ERROR] MCP Management is not healthy at $BASE_URL" >&2
  exit 1
fi

discover_local_cloud_project() {
  require_command docker
  local mongodb_container
  mongodb_container="$(
    docker compose -f "$ROOT_DIR/docker/compose.yml" ps -q mongodb 2>/dev/null || true
  )"
  if [[ -z "$mongodb_container" ]]; then
    return 1
  fi
  docker exec "$mongodb_container" mongosh \
    -u "${MONGODB_USER:-admin}" \
    -p "${MONGODB_PASSWORD:-admin}" \
    --authenticationDatabase admin \
    --quiet \
    --eval '
      const project = db.getSiblingDB("project_management_service").projects.findOne(
        {
          source_type: "cloud",
          status: "active",
          owner_user_id: {$type: "string"},
          id: {$type: "string"}
        },
        {_id: 0, id: 1, owner_user_id: 1}
      );
      if (project) {
        print(project.owner_user_id + "|" + project.id);
      }
    '
}

owner_user_id="${MCP_SMOKE_OWNER_USER_ID:-}"
project_id="${MCP_SMOKE_PROJECT_ID:-}"
if [[ -z "$owner_user_id" || -z "$project_id" ]]; then
  selection="$(discover_local_cloud_project || true)"
  if [[ -z "$selection" || "$selection" != *"|"* ]]; then
    echo "[ERROR] no active cloud project was found" >&2
    echo "[INFO] set MCP_SMOKE_OWNER_USER_ID and MCP_SMOKE_PROJECT_ID explicitly" >&2
    exit 1
  fi
  owner_user_id="${selection%%|*}"
  project_id="${selection#*|}"
fi

if [[ -z "$owner_user_id" || -z "$project_id" ]]; then
  echo "[ERROR] cloud smoke owner and project identities must be non-empty" >&2
  exit 1
fi

export MCP_SMOKE_OWNER_USER_ID="$owner_user_id"
export MCP_SMOKE_PROJECT_ID="$project_id"
export MCP_MANAGEMENT_SMOKE_BASE_URL="$BASE_URL"

echo "[INFO] smoke MCP Management cloud runtime"

node <<'NODE'
const crypto = require('crypto');

const baseUrl = process.env.MCP_MANAGEMENT_SMOKE_BASE_URL;
const internalSecret =
  process.env.MCP_MANAGEMENT_INTERNAL_API_SECRET ||
  'change_me_mcp_management_internal_secret';
const ownerUserId = process.env.MCP_SMOKE_OWNER_USER_ID;
const projectId = process.env.MCP_SMOKE_PROJECT_ID;
const primaryCaller = 'chatos';
const otherCaller = 'task-runner';

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function encodedJson(value) {
  return Buffer.from(JSON.stringify(value)).toString('base64url');
}

function issueInternalToken(caller, scope) {
  const now = Math.floor(Date.now() / 1000);
  const header = encodedJson({ alg: 'HS256', typ: 'JWT' });
  const payload = encodedJson({
    iss: caller,
    sub: caller,
    aud: 'mcp-management-service',
    scope,
    iat: now,
    exp: now + 60,
  });
  const signature = crypto
    .createHmac('sha256', internalSecret)
    .update(`${header}.${payload}`)
    .digest('base64url');
  return `${header}.${payload}.${signature}`;
}

async function readJson(response) {
  const text = await response.text();
  if (!text) {
    return null;
  }
  try {
    return JSON.parse(text);
  } catch {
    throw new Error(`endpoint returned non-JSON content with status ${response.status}`);
  }
}

async function internalRequest(path, scope, options = {}, caller = primaryCaller) {
  const response = await fetch(`${baseUrl}${path}`, {
    ...options,
    headers: {
      'content-type': 'application/json',
      'x-mcp-management-caller-service': caller,
      'x-mcp-management-internal-token': issueInternalToken(caller, scope),
      ...(options.headers || {}),
    },
  });
  return { response, body: await readJson(response) };
}

async function mcpRequest(runtimeUrl, runtimeToken, request) {
  const response = await fetch(runtimeUrl, {
    method: 'POST',
    headers: {
      authorization: `Bearer ${runtimeToken}`,
      'content-type': 'application/json',
    },
    body: JSON.stringify(request),
  });
  return { response, body: await readJson(response) };
}

function createSessionRequest(agentKey, owner = ownerUserId) {
  const identity = crypto.randomUUID();
  return {
    owner_user_id: owner,
    agent_key: agentKey,
    project_id: projectId,
    run_id: null,
    turn_id: `mcp-smoke-turn-${identity}`,
    task_id: null,
    task_profile: null,
    source_session_id: `mcp-smoke-session-${identity}`,
    source_user_message_id: `mcp-smoke-message-${identity}`,
    contact_agent_id: null,
    default_model_config_id: null,
    expected_project_task_ids: [],
    locale: 'zh-CN',
    requested_device_id: null,
    requested_sandbox_provider: null,
    sandbox_target: null,
  };
}

async function resolveSession(request) {
  return internalRequest('/api/internal/runtime/sessions/resolve', 'runtime.sessions.resolve', {
    method: 'POST',
    body: JSON.stringify(request),
  });
}

async function main() {
  const localOnly = await resolveSession(
    createSessionRequest('local_connector_command_approval_agent'),
  );
  assert(
    localOnly.response.status === 409,
    `local-only Agent unexpectedly entered MCP Management: ${localOnly.response.status}`,
  );
  console.log('[OK] local-only approval Agent is rejected by the cloud Tool Plane');

  const wrongOwner = await resolveSession(
    createSessionRequest('chatos_planning_agent', `${ownerUserId}-other`),
  );
  assert(
    !wrongOwner.response.ok,
    'a mismatched owner unexpectedly resolved another owner\'s cloud project',
  );
  console.log('[OK] project owner mismatch fails closed');

  let session = null;
  let closed = false;
  try {
    const created = await resolveSession(createSessionRequest('chatos_planning_agent'));
    assert(
      created.response.ok,
      `runtime session resolution failed with status ${created.response.status}`,
    );
    session = created.body;
    assert(session && session.session_id, 'runtime session id is missing');
    assert(session.runtime_token, 'runtime session token is missing');
    assert(session.mcp_server_url, 'runtime MCP URL is missing');
    assert(session.exposed_tool_count > 0, 'runtime session exposed no tools');
    const runtimeUrl = process.env.MCP_MANAGEMENT_SMOKE_RUNTIME_URL || `${baseUrl}/mcp`;
    console.log(
      `[OK] Runtime Session resolved with ${session.configured_mcp_count} MCP resources and ${session.exposed_tool_count} tools`,
    );

    const routes = await internalRequest(
      `/api/internal/runtime/sessions/${encodeURIComponent(session.session_id)}/routes`,
      'runtime.sessions.read',
      { method: 'GET' },
    );
    assert(routes.response.ok, `runtime route snapshot read failed: ${routes.response.status}`);
    assert(routes.body.owner_user_id === ownerUserId, 'runtime route owner identity drifted');
    assert(routes.body.project_id === projectId, 'runtime route project identity drifted');
    assert(
      routes.body.agent_key === 'chatos_planning_agent',
      'runtime route Agent identity drifted',
    );
    const taskRunnerRoute = routes.body.routes.find(
      (route) => route.resource_id === 'system_mcp_chatos_task_runner',
    );
    assert(taskRunnerRoute, 'Task Runner MCP route is missing');
    assert(
      taskRunnerRoute.provider_kind === 'internal_service' &&
        taskRunnerRoute.provider_ref === 'task_runner_service',
      'Task Runner MCP did not route to its owning internal service',
    );
    const serializedRoutes = JSON.stringify(routes.body);
    assert(
      !serializedRoutes.includes('/Users/') &&
        !serializedRoutes.includes('file://') &&
        !/[A-Za-z]:\\\\/.test(serializedRoutes),
      'runtime route snapshot exposed an absolute local path',
    );
    console.log('[OK] immutable route snapshot preserves owner, Agent, project and provider');

    const wrongCallerRead = await internalRequest(
      `/api/internal/runtime/sessions/${encodeURIComponent(session.session_id)}/routes`,
      'runtime.sessions.read',
      { method: 'GET' },
      otherCaller,
    );
    assert(
      wrongCallerRead.response.status === 403,
      'another caller service unexpectedly read the Runtime Session',
    );
    console.log('[OK] Runtime Session is isolated from another caller service');

    const toolsList = await mcpRequest(runtimeUrl, session.runtime_token, {
      jsonrpc: '2.0',
      id: 'smoke-tools-list',
      method: 'tools/list',
      params: {},
    });
    assert(toolsList.response.ok, `tools/list HTTP failed: ${toolsList.response.status}`);
    assert(!toolsList.body.error, `tools/list failed: ${JSON.stringify(toolsList.body.error)}`);
    const tools = toolsList.body.result?.tools || [];
    const listTasksTool = tools.find((tool) => tool.name.endsWith('_list_tasks'));
    assert(listTasksTool, 'namespaced Task Runner list_tasks tool is missing');
    console.log(`[OK] aggregated tools/list returned ${tools.length} namespaced tools`);

    const tamperedToken = `${session.runtime_token.slice(0, -1)}${
      session.runtime_token.endsWith('a') ? 'b' : 'a'
    }`;
    const tampered = await mcpRequest(runtimeUrl, tamperedToken, {
      jsonrpc: '2.0',
      id: 'smoke-tampered-token',
      method: 'tools/list',
      params: {},
    });
    assert(tampered.body?.error, 'tampered Runtime Token was unexpectedly accepted');
    console.log('[OK] tampered Runtime Token is rejected');

    const toolCall = await mcpRequest(runtimeUrl, session.runtime_token, {
      jsonrpc: '2.0',
      id: 'smoke-tools-call',
      method: 'tools/call',
      params: {
        name: listTasksTool.name,
        arguments: { limit: 1, offset: 0 },
      },
    });
    assert(toolCall.response.ok, `tools/call HTTP failed: ${toolCall.response.status}`);
    assert(!toolCall.body.error, `tools/call failed: ${JSON.stringify(toolCall.body.error)}`);
    assert(toolCall.body.result, 'tools/call returned no result');
    console.log('[OK] real MCP Management -> Task Runner tools/call completed');

    const wrongCallerClose = await internalRequest(
      `/api/internal/runtime/sessions/${encodeURIComponent(session.session_id)}/close`,
      'runtime.sessions.close',
      { method: 'POST', body: '{}' },
      otherCaller,
    );
    assert(
      wrongCallerClose.response.status === 403,
      'another caller service unexpectedly closed the Runtime Session',
    );

    const close = await internalRequest(
      `/api/internal/runtime/sessions/${encodeURIComponent(session.session_id)}/close`,
      'runtime.sessions.close',
      { method: 'POST', body: '{}' },
    );
    assert(close.response.ok && close.body?.closed === true, 'Runtime Session close failed');
    closed = true;

    const afterClose = await mcpRequest(runtimeUrl, session.runtime_token, {
      jsonrpc: '2.0',
      id: 'smoke-after-close',
      method: 'tools/list',
      params: {},
    });
    assert(afterClose.body?.error, 'closed Runtime Session remained callable');
    console.log('[OK] closed Runtime Session cannot be reused');
  } finally {
    if (session?.session_id && !closed) {
      await internalRequest(
        `/api/internal/runtime/sessions/${encodeURIComponent(session.session_id)}/close`,
        'runtime.sessions.close',
        { method: 'POST', body: '{}' },
      ).catch(() => {});
    }
  }
}

main()
  .then(() => console.log('[OK] MCP Management cloud runtime smoke passed'))
  .catch((error) => {
    console.error(`[ERROR] ${error.message}`);
    process.exitCode = 1;
  });
NODE
