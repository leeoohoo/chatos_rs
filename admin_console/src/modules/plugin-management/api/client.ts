// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentPromptCompleteness,
  AgentPromptVersionRecord,
  AgentPromptVersionSummary,
  GenerateAgentPromptResponse,
  AgentProviderPromptRecord,
  AgentMcpBindingsResponse,
  AdminAiModelConfig,
  BindingConditions,
  CurrentUser,
  ListResponse,
  McpRecord,
  McpDescriptorResponse,
  McpProviderSkill,
  OptimizeProviderSkillResponse,
  OptimizeProviderSkillStreamEvent,
  ResourceCheckRecord,
  RuntimeCapabilitiesResponse,
  SystemAgentRecord,
} from '../types';
import type {
  PluginAuditLogRecord,
  PluginCatalogListItem,
  PluginCatalogSyncResponse,
  PluginInstallationRecord,
  PluginMarketplaceRecord,
  PluginOAuthConnectionRecord,
  PluginPublisherRecord,
  PluginPackageAnalysis,
  PublishUploadedPluginResponse,
  PluginReleaseRecord,
} from '../pluginTypes';

import {
  createJsonApiClient,
  readApiErrorMessage,
  withQuery,
  type QueryValue,
} from '@chatos/frontend-runtime';
import {
  clearAuthToken as clearSharedAuthToken,
  getAuthToken as getSharedAuthToken,
} from '../../../shared/auth/tokenStore';
import {
  ADMIN_SERVICE_BASES,
  buildAdminServiceUrl,
  stripBackendApiPrefix,
} from '../../../shared/api/servicePaths';
import {
  filterRetiredAgentMcpBindings,
  filterRetiredMcpList,
  filterRetiredRuntimeCapabilities,
  isRetiredTaskManagerMcpId,
} from '../retiredMcps';

const API_BASE_URL = ADMIN_SERVICE_BASES.pluginManagement;

export function getAuthToken(): string | null {
  return getSharedAuthToken();
}

export function clearAuthToken(): void {
  clearSharedAuthToken();
}

function buildApiUrl(path: string): string {
  return buildAdminServiceUrl(API_BASE_URL, path);
}

const rawRequest = createJsonApiClient({
  baseUrl: API_BASE_URL,
  timeoutMs: 30_000,
  getAuthToken,
  onUnauthorized: clearAuthToken,
});

const request = <T,>(path: string, init?: RequestInit) =>
  rawRequest<T>(stripBackendApiPrefix(path), init);

async function requestMultipart<T>(path: string, body: FormData): Promise<T> {
  const headers = new Headers();
  const token = getAuthToken();
  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
  }
  const response = await fetch(buildApiUrl(path), { method: 'POST', headers, body });
  if (!response.ok) {
    if (response.status === 401) clearAuthToken();
    throw new Error(await readApiErrorMessage(response));
  }
  return response.json() as Promise<T>;
}

export const api = {
  currentUser: () => request<CurrentUser>('/api/auth/me'),
  listMcps: (params?: Record<string, QueryValue>) =>
    request<ListResponse<McpRecord>>(withQuery('/api/mcps', params || {})).then(
      filterRetiredMcpList,
    ),
  createMcp: (payload: unknown) =>
    request<McpRecord>('/api/mcps', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateMcp: (id: string, payload: unknown) =>
    request<McpRecord>(`/api/mcps/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteMcp: (id: string) =>
    request<void>(`/api/mcps/${id}`, {
      method: 'DELETE',
    }),
  checkMcp: (id: string) =>
    request<ResourceCheckRecord>(`/api/mcps/${id}/check`, {
      method: 'POST',
    }),
  getMcpDescriptor: (id: string) =>
    request<McpDescriptorResponse>(`/api/mcps/${id}/descriptor`),
  listAdminAiModels: () => request<AdminAiModelConfig[]>('/api/admin/ai-models'),
  optimizeMcpProviderSkill: (
    id: string,
    payload: { model_config_id: string; skill_id: string; requirement: string },
  ) =>
    request<OptimizeProviderSkillResponse>(`/api/mcps/${id}/provider-skills/optimize`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  optimizeMcpProviderSkillStream: (
    id: string,
    payload: { model_config_id: string; skill_id: string; requirement: string },
    onEvent: (event: OptimizeProviderSkillStreamEvent) => void,
    signal?: AbortSignal,
  ) =>
    requestSse(
      `/api/mcps/${id}/provider-skills/optimize/stream`,
      payload,
      onEvent,
      signal,
    ),
  updateMcpProviderSkill: (id: string, skillId: string, instructions: string) =>
    request<McpProviderSkill>(
      `/api/mcps/${id}/provider-skills/${encodeURIComponent(skillId)}`,
      {
        method: 'PUT',
        body: JSON.stringify({ instructions }),
      },
    ),
  listPluginMarketplaces: () =>
    request<ListResponse<PluginMarketplaceRecord>>('/api/plugin-marketplaces'),
  listPluginCatalog: (params?: Record<string, QueryValue>) =>
    request<ListResponse<PluginCatalogListItem>>(
      withQuery('/api/plugins/catalog', params || {}),
    ),
  createPluginMarketplace: (payload: unknown) =>
    request<PluginMarketplaceRecord>('/api/plugin-marketplaces', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateAdminPluginMarketplace: (marketplaceId: string, payload: unknown) =>
    request<PluginMarketplaceRecord>(
      `/api/admin/plugin-marketplaces/${encodeURIComponent(marketplaceId)}`,
      {
        method: 'PATCH',
        body: JSON.stringify(payload),
      },
    ),
  syncPluginMarketplace: (marketplaceId: string) =>
    request<PluginCatalogSyncResponse>(
      `/api/plugin-marketplaces/${encodeURIComponent(marketplaceId)}/sync`,
      { method: 'POST' },
    ),
  listPluginPublishers: (params?: Record<string, QueryValue>) =>
    request<ListResponse<PluginPublisherRecord>>(withQuery('/api/plugin-publishers', params || {})),
  submitPluginPublisher: (payload: unknown) =>
    request<PluginPublisherRecord>('/api/plugin-publishers', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listAdminPluginPublishers: (params?: Record<string, QueryValue>) =>
    request<ListResponse<PluginPublisherRecord>>(
      withQuery('/api/admin/plugin-publishers', params || {}),
    ),
  reviewAdminPluginPublisher: (publisherRecordId: string, payload: unknown) =>
    request<PluginPublisherRecord>(
      `/api/admin/plugin-publishers/${encodeURIComponent(publisherRecordId)}/review`,
      {
        method: 'PATCH',
        body: JSON.stringify(payload),
      },
    ),
  listAdminPlugins: (params?: Record<string, QueryValue>) =>
    request<ListResponse<PluginCatalogListItem>>(withQuery('/api/admin/plugins', params || {})),
  analyzePluginPackage: (payload: FormData) =>
    requestMultipart<PluginPackageAnalysis>('/api/admin/plugin-package/analyze', payload),
  publishUploadedPlugin: (payload: unknown) =>
    request<PublishUploadedPluginResponse>('/api/admin/plugin-package/publish', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  listPluginReleases: (pluginId: string) =>
    request<ListResponse<PluginReleaseRecord>>(
      `/api/admin/plugins/${encodeURIComponent(pluginId)}/releases`,
    ),
  listVisiblePluginReleases: (pluginId: string) =>
    request<ListResponse<PluginReleaseRecord>>(
      `/api/plugins/${encodeURIComponent(pluginId)}/releases`,
    ),
  revokePluginRelease: (releaseId: string) =>
    request<PluginReleaseRecord>(
      `/api/admin/plugin-releases/${encodeURIComponent(releaseId)}/revoke`,
      { method: 'POST' },
    ),
  listPluginAudit: (params?: Record<string, QueryValue>) =>
    request<ListResponse<PluginAuditLogRecord>>(
      withQuery('/api/admin/plugin-audit', params || {}),
    ),
  listInstalledPlugins: (params: Record<string, QueryValue>) =>
    request<ListResponse<PluginInstallationRecord>>(withQuery('/api/plugins/installed', params)),
  listPluginOAuthConnections: (pluginId: string, params: Record<string, QueryValue>) =>
    request<ListResponse<PluginOAuthConnectionRecord>>(
      withQuery(`/api/plugins/${encodeURIComponent(pluginId)}/oauth`, params),
    ),
  listSystemAgents: () => request<SystemAgentRecord[]>('/api/system-agents'),
  createSystemAgent: (payload: unknown) =>
    request<SystemAgentRecord>('/api/system-agents', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateSystemAgent: (agentKey: string, payload: unknown) =>
    request<SystemAgentRecord>(`/api/system-agents/${agentKey}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  getAgentMcpBindings: (agentKey: string) =>
    request<AgentMcpBindingsResponse>(`/api/system-agents/${agentKey}/mcp-bindings`).then(
      filterRetiredAgentMcpBindings,
    ),
  updateAgentMcpBindings: (
    agentKey: string,
    bindings: Array<{
      mcp_id: string;
      mode: string;
      conditions?: BindingConditions;
      tool_allowlist?: string[];
      tool_blocklist?: string[];
    }>,
  ) =>
    request<AgentMcpBindingsResponse>(`/api/system-agents/${agentKey}/mcp-bindings`, {
      method: 'PUT',
      body: JSON.stringify({
        bindings: bindings.filter((binding) => !isRetiredTaskManagerMcpId(binding.mcp_id)),
      }),
    }).then(filterRetiredAgentMcpBindings),
  listAgentProviderPrompts: (agentKey: string) =>
    request<AgentProviderPromptRecord[]>(
      `/api/system-agents/${encodeURIComponent(agentKey)}/provider-prompts`,
    ),
  listAgentPromptVersions: (agentKey: string) =>
    request<AgentPromptVersionSummary[]>(
      `/api/system-agents/${encodeURIComponent(agentKey)}/prompt-versions`,
    ),
  getAgentPromptVersion: (agentKey: string, bundleVersion: number) =>
    request<AgentPromptVersionRecord>(
      `/api/system-agents/${encodeURIComponent(agentKey)}/prompt-versions/${bundleVersion}`,
    ),
  updateAgentProviderPromptDraft: (
    agentKey: string,
    profile: string,
    vendor: string,
    content: string,
    expectedUpdatedAt?: string,
  ) =>
    request<AgentProviderPromptRecord>(
      withQuery(
        `/api/system-agents/${encodeURIComponent(agentKey)}/provider-prompts/${encodeURIComponent(vendor)}/draft`,
        { profile },
      ),
      {
        method: 'PUT',
        body: JSON.stringify({ content, expected_updated_at: expectedUpdatedAt }),
      },
    ),
  publishAgentProviderPrompt: (agentKey: string, profile: string, vendor: string) =>
    request<AgentProviderPromptRecord>(
      withQuery(
        `/api/system-agents/${encodeURIComponent(agentKey)}/provider-prompts/${encodeURIComponent(vendor)}/publish`,
        { profile },
      ),
      { method: 'POST', body: JSON.stringify({}) },
    ),
  generateAgentProviderPrompt: (
    agentKey: string,
    profile: string,
    vendor: string,
    payload: { model_config_id: string; requirement: string; current_content: string },
  ) => request<GenerateAgentPromptResponse>(withQuery(
    `/api/system-agents/${encodeURIComponent(agentKey)}/provider-prompts/${encodeURIComponent(vendor)}/generate`,
    { profile },
  ),
    { method: 'POST', body: JSON.stringify(payload) },
  ),
  agentPromptCompleteness: () =>
    request<AgentPromptCompleteness[]>('/api/system-agents/prompt-completeness'),
  resolveAgentCapabilities: (params: Record<string, QueryValue>) =>
    request<RuntimeCapabilitiesResponse>(withQuery('/api/runtime/agent-capabilities', params)).then(
      filterRetiredRuntimeCapabilities,
    ),
};

async function requestSse(
  path: string,
  payload: unknown,
  onEvent: (event: OptimizeProviderSkillStreamEvent) => void,
  signal?: AbortSignal,
): Promise<void> {
  const headers = new Headers({
    Accept: 'text/event-stream',
    'Cache-Control': 'no-cache',
    'Content-Type': 'application/json',
  });
  const token = getAuthToken();
  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
  }
  const response = await fetch(buildApiUrl(path), {
    method: 'POST',
    headers,
    body: JSON.stringify(payload),
    signal,
  });
  if (!response.ok) {
    let detail = response.statusText;
    try {
      const body = (await response.json()) as { error?: string };
      detail = body.error || detail;
    } catch {
      // keep status text
    }
    throw new Error(detail);
  }
  if (!response.body) {
    throw new Error('Streaming response body is unavailable');
  }
  const contentType = response.headers.get('content-type') || '';
  if (!contentType.toLowerCase().includes('text/event-stream')) {
    throw new Error(`Expected an SSE response but received ${contentType || 'an unknown content type'}`);
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let streamError: string | null = null;
  let receivedDone = false;

  const processBlock = (block: string) => {
    const data = block
      .split('\n')
      .filter((line) => line.startsWith('data:'))
      .map((line) => line.slice(5).trimStart())
      .join('\n')
      .trim();
    if (!data) {
      return;
    }
    const event = JSON.parse(data) as OptimizeProviderSkillStreamEvent;
    onEvent(event);
    if (event.type === 'error') {
      streamError = event.message;
    } else if (event.type === 'done') {
      receivedDone = true;
    }
  };

  while (true) {
    const { value, done } = await reader.read();
    if (done) {
      break;
    }
    buffer += decoder.decode(value, { stream: true });
    buffer = buffer.replace(/\r\n/g, '\n');
    let boundary = buffer.indexOf('\n\n');
    while (boundary >= 0) {
      processBlock(buffer.slice(0, boundary));
      buffer = buffer.slice(boundary + 2);
      boundary = buffer.indexOf('\n\n');
    }
  }
  buffer += decoder.decode();
  if (buffer.trim()) {
    processBlock(buffer);
  }
  if (streamError) {
    throw new Error(streamError);
  }
  if (!receivedDone) {
    throw new Error('AI stream ended before the final result was received');
  }
}
