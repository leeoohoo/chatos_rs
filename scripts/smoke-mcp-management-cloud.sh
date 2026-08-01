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

discover_project_by_source() {
  local source_type="$1"
  require_command docker
  local mongodb_container
  mongodb_container="$(
    docker compose -f "$ROOT_DIR/docker/compose.yml" ps -q mongodb 2>/dev/null || true
  )"
  if [[ -z "$mongodb_container" ]]; then
    return 1
  fi
  docker exec -e MCP_SMOKE_PROJECT_SOURCE_TYPE="$source_type" "$mongodb_container" mongosh \
    -u "${MONGODB_USER:-admin}" \
    -p "${MONGODB_PASSWORD:-admin}" \
    --authenticationDatabase admin \
    --quiet \
    --eval '
      const project = db.getSiblingDB("project_management_service").projects.findOne(
        {
          source_type: process.env.MCP_SMOKE_PROJECT_SOURCE_TYPE,
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

discover_local_project_with_sandbox() {
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
      const projectDb = db.getSiblingDB("project_management_service");
      const environments = projectDb.project_runtime_environments.find(
        {
          sandbox_enabled: true,
          file_provider: "local_connector",
          sandbox_provider: "local_connector",
          project_id: {$type: "string"}
        },
        {_id: 0, project_id: 1}
      ).sort({updated_at: -1}).toArray();
      for (const environment of environments) {
        const project = projectDb.projects.findOne(
          {
            id: environment.project_id,
            source_type: "local_connector",
            status: "active",
            owner_user_id: {$type: "string"}
          },
          {_id: 0, id: 1, owner_user_id: 1}
        );
        if (project) {
          print(project.owner_user_id + "|" + project.id);
          break;
        }
      }
    '
}

local_project_has_live_sandbox() {
  local owner_user_id="$1"
  local project_id="$2"
  require_command docker
  local mongodb_container
  mongodb_container="$(
    docker compose -f "$ROOT_DIR/docker/compose.yml" ps -q mongodb 2>/dev/null || true
  )"
  if [[ -z "$mongodb_container" ]]; then
    return 1
  fi
  docker exec \
    -e MCP_SMOKE_LOCAL_OWNER_USER_ID="$owner_user_id" \
    -e MCP_SMOKE_LOCAL_PROJECT_ID="$project_id" \
    "$mongodb_container" mongosh \
    -u "${MONGODB_USER:-admin}" \
    -p "${MONGODB_PASSWORD:-admin}" \
    --authenticationDatabase admin \
    --quiet \
    --eval '
      const projectDb = db.getSiblingDB("project_management_service");
      const connectorDb = db.getSiblingDB("local_connector_service");
      const project = projectDb.projects.findOne({
        id: process.env.MCP_SMOKE_LOCAL_PROJECT_ID,
        owner_user_id: process.env.MCP_SMOKE_LOCAL_OWNER_USER_ID,
        source_type: "local_connector",
        status: "active"
      });
      const environment = project && projectDb.project_runtime_environments.findOne({
        project_id: project.id,
        sandbox_enabled: true,
        file_provider: "local_connector",
        sandbox_provider: "local_connector"
      });
      const match = project && /^local:\/\/connector\/([^/]+)\/([^/]+)(?:\/|$)/.exec(
        project.root_path || ""
      );
      if (!project || !environment || !match) {
        quit(1);
      }
      let deviceId;
      let workspaceId;
      try {
        deviceId = decodeURIComponent(match[1]);
        workspaceId = decodeURIComponent(match[2]);
      } catch {
        quit(1);
      }
      const pairing = connectorDb.local_connector_sandbox_pairings.findOne({
        owner_user_id: project.owner_user_id,
        device_id: deviceId,
        workspace_id: workspaceId,
        enabled: true,
        sandbox_readiness: /^ready$/i
      });
      const liveSession = connectorDb.local_connector_sessions.find({
        owner_user_id: project.owner_user_id,
        device_id: deviceId,
        status: "connected"
      }).toArray().some((session) => {
        const expiresAt = Date.parse(session.expires_at || "");
        return Number.isFinite(expiresAt) && expiresAt > Date.now();
      });
      if (!pairing || !liveSession) {
        quit(1);
      }
      print("live");
    ' >/dev/null
}

owner_user_id="${MCP_SMOKE_OWNER_USER_ID:-}"
project_id="${MCP_SMOKE_PROJECT_ID:-}"
if [[ -z "$owner_user_id" || -z "$project_id" ]]; then
  selection="$(discover_project_by_source cloud || true)"
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

local_owner_user_id="${MCP_SMOKE_LOCAL_OWNER_USER_ID:-}"
local_project_id="${MCP_SMOKE_LOCAL_PROJECT_ID:-}"
if [[ -z "$local_owner_user_id" || -z "$local_project_id" ]]; then
  local_selection="$(discover_local_project_with_sandbox || true)"
  if [[ -n "$local_selection" && "$local_selection" == *"|"* ]]; then
    local_owner_user_id="${local_selection%%|*}"
    local_project_id="${local_selection#*|}"
  fi
fi
local_runtime_live=0
if [[ -n "$local_owner_user_id" && -n "$local_project_id" ]] \
  && local_project_has_live_sandbox "$local_owner_user_id" "$local_project_id"; then
  local_runtime_live=1
fi
export MCP_SMOKE_LOCAL_OWNER_USER_ID="$local_owner_user_id"
export MCP_SMOKE_LOCAL_PROJECT_ID="$local_project_id"
export MCP_SMOKE_LOCAL_RUNTIME_LIVE="$local_runtime_live"

echo "[INFO] smoke MCP Management cloud runtime"

node <<'NODE'
const crypto = require('crypto');

const baseUrl = process.env.MCP_MANAGEMENT_SMOKE_BASE_URL;
const internalSecret =
  process.env.MCP_MANAGEMENT_INTERNAL_API_SECRET ||
  'change_me_mcp_management_internal_secret';
const projectServiceBaseUrl = (
  process.env.MCP_MANAGEMENT_PROJECT_SERVICE_BASE_URL ||
  process.env.PROJECT_SERVICE_BASE_URL ||
  'http://127.0.0.1:39210'
).replace(/\/$/, '');
const projectServiceSecret =
  process.env.MCP_MANAGEMENT_PROJECT_SERVICE_INTERNAL_API_SECRET ||
  'change_me_mcp_management_project_service_secret';
const ownerUserId = process.env.MCP_SMOKE_OWNER_USER_ID;
const projectId = process.env.MCP_SMOKE_PROJECT_ID;
const localOwnerUserId = process.env.MCP_SMOKE_LOCAL_OWNER_USER_ID;
const localProjectId = process.env.MCP_SMOKE_LOCAL_PROJECT_ID;
const localRuntimeLive = process.env.MCP_SMOKE_LOCAL_RUNTIME_LIVE === '1';
const primaryCaller = 'chatos';
const otherCaller = 'task-runner';
const projectServiceCaller = 'project-service';

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

function assertInternalRoute(routes, resourceId, providerRef, label) {
  const route = routes.find((item) => item.resource_id === resourceId);
  assert(route, `${label} MCP route is missing`);
  assert(
    route.provider_kind === 'internal_service' && route.provider_ref === providerRef,
    `${label} MCP did not route to ${providerRef}`,
  );
  return route;
}

function encodedJson(value) {
  return Buffer.from(JSON.stringify(value)).toString('base64url');
}

function issueSignedToken(caller, audience, scope, secret) {
  const now = Math.floor(Date.now() / 1000);
  const header = encodedJson({ alg: 'HS256', typ: 'JWT' });
  const payload = encodedJson({
    iss: caller,
    sub: caller,
    aud: audience,
    scope,
    iat: now,
    exp: now + 60,
  });
  const signature = crypto
    .createHmac('sha256', secret)
    .update(`${header}.${payload}`)
    .digest('base64url');
  return `${header}.${payload}.${signature}`;
}

function issueInternalToken(caller, scope) {
  return issueSignedToken(
    caller,
    'mcp-management-service',
    scope,
    internalSecret,
  );
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

async function projectExecutionContext(project, owner) {
  const caller = 'mcp-management-service';
  const token = issueSignedToken(
    caller,
    'project-service',
    'project.execution_context.read',
    projectServiceSecret,
  );
  const response = await fetch(
    `${projectServiceBaseUrl}/api/internal/projects/${encodeURIComponent(
      project,
    )}/execution-context?owner_user_id=${encodeURIComponent(owner)}`,
    {
      method: 'GET',
      headers: {
        'x-project-service-caller': caller,
        'x-project-service-internal-token': token,
        'x-project-service-sync-secret': projectServiceSecret,
      },
    },
  );
  return { response, body: await readJson(response) };
}

async function assertWorkspaceRoute(project, owner, expectedProvider) {
  const contextResult = await projectExecutionContext(project, owner);
  assert(
    contextResult.response.ok,
    `project execution context failed with status ${contextResult.response.status}`,
  );
  const context = contextResult.body;
  assert(context.project_id === project, 'project execution context id drifted');
  assert(context.owner_user_id === owner, 'project execution context owner drifted');
  assert(
    context.workspace_provider === expectedProvider,
    `workspace provider drifted: expected ${expectedProvider}, received ${context.workspace_provider}`,
  );
  const resolved = await internalRequest('/api/internal/routes/resolve', 'routes.resolve', {
    method: 'POST',
    body: JSON.stringify({
      context,
      resources: [
        {
          resource_id: 'builtin_code_maintainer_read',
          server_name: 'code_maintainer_read',
          resource_kind: 'system',
          system_key: 'code_maintainer_read',
          execution_host: null,
          provider_ref: null,
          required: true,
          allow_writes: false,
        },
      ],
    }),
  });
  assert(resolved.response.ok, `workspace route resolve failed: ${resolved.response.status}`);
  assert(
    Array.isArray(resolved.body.routes) && resolved.body.routes.length === 1,
    'workspace route resolve returned an unexpected route set',
  );
  const route = resolved.body.routes[0];
  assert(
    route.provider_kind === expectedProvider,
    `workspace route used ${route.provider_kind} instead of ${expectedProvider}`,
  );
  assert(
    !resolved.body.unavailable_required_mcps?.length,
    'required workspace MCP was unavailable',
  );
  const serialized = JSON.stringify(resolved.body);
  assert(
    !serialized.includes('/Users/') &&
      !serialized.includes('file://') &&
      !/[A-Za-z]:\\\\/.test(serialized),
    'workspace route preview exposed an absolute local path',
  );
  if (expectedProvider === 'harness') {
    assert(
      route.provider_ref === `project:${project}@${context.revision}`,
      'Harness route is not pinned to the authoritative project revision',
    );
  } else if (expectedProvider === 'local_connector') {
    assert(context.workspace?.device_id, 'local project context has no device id');
    assert(context.workspace?.workspace_id, 'local project context has no workspace id');
    assert(
      route.provider_ref ===
        `device:${context.workspace.device_id}/workspace:${context.workspace.workspace_id}`,
      'Local Connector route is not pinned to the authoritative device and workspace',
    );
  }
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

async function callMcpTool(runtimeUrl, runtimeToken, requestId, tool, args, label) {
  const call = await mcpRequest(runtimeUrl, runtimeToken, {
    jsonrpc: '2.0',
    id: requestId,
    method: 'tools/call',
    params: { name: tool.name, arguments: args },
  });
  assert(call.response.ok, `${label} tools/call HTTP failed: ${call.response.status}`);
  assert(!call.body.error, `${label} tools/call failed: ${JSON.stringify(call.body.error)}`);
  assert(call.body.result, `${label} tools/call returned no result`);
  console.log(`[OK] real MCP Management -> ${label} tools/call completed`);
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

function createProjectAgentSessionRequest(project, owner) {
  return {
    owner_user_id: owner,
    agent_key: 'project_management_agent',
    project_id: project,
    run_id: `mcp-smoke-project-environment-${crypto.randomUUID()}`,
    turn_id: null,
    task_id: null,
    task_profile: null,
    source_session_id: null,
    source_user_message_id: null,
    contact_agent_id: null,
    default_model_config_id: 'mcp-smoke-project-environment-model',
    expected_project_task_ids: [],
    locale: 'zh-CN',
    requested_device_id: null,
    requested_sandbox_provider: null,
    sandbox_target: null,
  };
}

async function resolveSession(request, caller = primaryCaller) {
  return internalRequest('/api/internal/runtime/sessions/resolve', 'runtime.sessions.resolve', {
    method: 'POST',
    body: JSON.stringify(request),
  }, caller);
}

async function assertProjectAgentRuntime(
  project,
  owner,
  expectedWorkspaceProvider,
  expectedSandboxProvider,
) {
  const contextResult = await projectExecutionContext(project, owner);
  assert(contextResult.response.ok, 'project Agent execution context lookup failed');
  assert(
    contextResult.body.workspace_provider === expectedWorkspaceProvider,
    `project Agent workspace provider drifted from ${expectedWorkspaceProvider}`,
  );
  assert(
    contextResult.body.sandbox_provider === expectedSandboxProvider,
    `project Agent sandbox provider drifted from ${expectedSandboxProvider}`,
  );

  let session = null;
  try {
    const created = await resolveSession(
      createProjectAgentSessionRequest(project, owner),
      projectServiceCaller,
    );
    assert(
      created.response.ok,
      `Project Environment Runtime Session failed: ${created.response.status}`,
    );
    session = created.body;
    const routes = await internalRequest(
      `/api/internal/runtime/sessions/${encodeURIComponent(session.session_id)}/routes`,
      'runtime.sessions.read',
      { method: 'GET' },
      projectServiceCaller,
    );
    assert(routes.response.ok, 'Project Environment route snapshot read failed');
    assert(routes.body.owner_user_id === owner, 'Project Environment owner drifted');
    assert(routes.body.project_id === project, 'Project Environment project drifted');
    assert(
      routes.body.agent_key === 'project_management_agent',
      'Project Environment Agent identity drifted',
    );
    const workspaceRoute = routes.body.routes.find(
      (route) => route.resource_id === 'builtin_code_maintainer_read',
    );
    assert(workspaceRoute, 'Project Environment CodeMaintainerRead route is missing');
    assert(
      workspaceRoute.provider_kind === expectedWorkspaceProvider,
      `Project Environment workspace tool used ${workspaceRoute.provider_kind}`,
    );
    assertInternalRoute(
      routes.body.routes,
      'system_mcp_project_environment',
      'project_management_service',
      'Project Environment',
    );
    const sandboxImagesRoute = routes.body.routes.find(
      (route) => route.resource_id === 'system_mcp_sandbox_images',
    );
    assert(sandboxImagesRoute, 'Sandbox Images route is missing');
    const expectedSandboxProviderKind = expectedSandboxProvider === 'cloud'
      ? 'cloud_sandbox'
      : 'local_connector';
    assert(
      sandboxImagesRoute.provider_kind === expectedSandboxProviderKind,
      `Sandbox Images used ${sandboxImagesRoute.provider_kind} instead of ${expectedSandboxProviderKind}`,
    );
    if (expectedSandboxProvider === 'cloud') {
      assert(
        sandboxImagesRoute.provider_ref === 'sandbox-images:cloud',
        'cloud Sandbox Images route is not pinned to Sandbox Manager',
      );
    } else {
      assert(
        sandboxImagesRoute.provider_ref?.startsWith('sandbox-images:local:'),
        'local Sandbox Images route is not pinned to the authoritative pairing',
      );
    }

    const runtimeUrl = process.env.MCP_MANAGEMENT_SMOKE_RUNTIME_URL || `${baseUrl}/mcp`;
    const toolsList = await mcpRequest(runtimeUrl, session.runtime_token, {
      jsonrpc: '2.0',
      id: `smoke-project-environment-tools-${expectedWorkspaceProvider}`,
      method: 'tools/list',
      params: {},
    });
    assert(!toolsList.body?.error, 'Project Environment tools/list failed');
    const imageCatalogTool = (toolsList.body?.result?.tools || []).find(
      (tool) => tool.name.endsWith('_get_image_catalog'),
    );
    assert(imageCatalogTool, 'Sandbox Images get_image_catalog tool is missing');
    await callMcpTool(
      runtimeUrl,
      session.runtime_token,
      `smoke-sandbox-images-${expectedWorkspaceProvider}`,
      imageCatalogTool,
      {},
      `${expectedWorkspaceProvider} Sandbox Images`,
    );
    console.log(
      `[OK] Project Environment Agent routes ${expectedWorkspaceProvider} workspace and ${expectedSandboxProvider} sandbox programmatically`,
    );
  } finally {
    if (session?.session_id) {
      await internalRequest(
        `/api/internal/runtime/sessions/${encodeURIComponent(session.session_id)}/close`,
        'runtime.sessions.close',
        { method: 'POST', body: '{}' },
        projectServiceCaller,
      ).catch(() => {});
    }
  }
}

async function assertLocalProjectAgentFailsClosed(project, owner) {
  const contextResult = await projectExecutionContext(project, owner);
  assert(contextResult.response.ok, 'offline local Project Context lookup failed');
  assert(
    contextResult.body.workspace_provider === 'local_connector',
    'offline local project workspace escaped Local Connector routing',
  );
  assert(
    contextResult.body.sandbox_provider === 'local_connector',
    'offline local project sandbox policy drifted',
  );
  const created = await resolveSession(
    createProjectAgentSessionRequest(project, owner),
    projectServiceCaller,
  );
  assert(
    created.response.status === 409,
    `offline local Project Environment Runtime Session did not fail closed: ${created.response.status}`,
  );
  assert(
    created.body?.error?.includes('no active enabled and ready sandbox pairing'),
    `offline local Project Environment failure was ambiguous: ${JSON.stringify(created.body)}`,
  );
  console.log(
    '[OK] offline local Project Environment Runtime Session fails closed without a cloud fallback',
  );
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

  await assertWorkspaceRoute(projectId, ownerUserId, 'harness');
  console.log('[OK] cloud project CodeMaintainerRead is pinned to Harness');
  await assertProjectAgentRuntime(projectId, ownerUserId, 'harness', 'cloud');

  if (localOwnerUserId && localProjectId) {
    await assertWorkspaceRoute(localProjectId, localOwnerUserId, 'local_connector');
    console.log(
      '[OK] local project CodeMaintainerRead is pinned to its Local Connector workspace',
    );
    if (localRuntimeLive) {
      await assertProjectAgentRuntime(
        localProjectId,
        localOwnerUserId,
        'local_connector',
        'local_connector',
      );
    } else {
      await assertLocalProjectAgentFailsClosed(localProjectId, localOwnerUserId);
    }
  } else {
    console.log('[INFO] no active local project found; Local Connector route smoke skipped');
  }

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
    assertInternalRoute(
      routes.body.routes,
      'system_mcp_chatos_task_runner',
      'task_runner_service',
      'Task Runner',
    );
    assertInternalRoute(
      routes.body.routes,
      'builtin_project_management',
      'project_management_service',
      'Project Management',
    );
    assertInternalRoute(
      routes.body.routes,
      'builtin_notepad',
      'chatos',
      'Notepad',
    );
    const serializedRoutes = JSON.stringify(routes.body);
    assert(
      !serializedRoutes.includes('/Users/') &&
        !serializedRoutes.includes('file://') &&
        !/[A-Za-z]:\\\\/.test(serializedRoutes),
      'runtime route snapshot exposed an absolute local path',
    );
    console.log(
      `[INFO] runtime routes: ${routes.body.routes
        .map(
          (route) =>
            `${route.resource_id}=${route.provider_kind}:${route.provider_ref || 'none'}`,
        )
        .join(', ')}`,
    );
    console.log(
      '[OK] immutable route snapshot preserves owner, Agent, project and service ownership',
    );

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
    const listRequirementsTool = tools.find((tool) =>
      tool.name.endsWith('_list_requirements'),
    );
    assert(
      listRequirementsTool,
      'namespaced Project Management list_requirements tool is missing',
    );
    const listNotepadFoldersTool = tools.find((tool) =>
      tool.name.endsWith('_list_folders'),
    );
    assert(listNotepadFoldersTool, 'namespaced Notepad list_folders tool is missing');
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

    await callMcpTool(
      runtimeUrl,
      session.runtime_token,
      'smoke-task-list-call',
      listTasksTool,
      { limit: 1, offset: 0 },
      'Task Runner',
    );
    await callMcpTool(
      runtimeUrl,
      session.runtime_token,
      'smoke-project-requirements-call',
      listRequirementsTool,
      { limit: 1, offset: 0 },
      'Project Management',
    );
    await callMcpTool(
      runtimeUrl,
      session.runtime_token,
      'smoke-notepad-folders-call',
      listNotepadFoldersTool,
      {},
      'ChatOS Notepad',
    );

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
