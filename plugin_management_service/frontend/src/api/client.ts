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
  CurrentUser,
  ListResponse,
  LoginPayload,
  LoginResponse,
  McpRecord,
  McpDescriptorResponse,
  McpProviderSkill,
  OptimizeProviderSkillResponse,
  OptimizeProviderSkillStreamEvent,
  ResourceCheckRecord,
  RuntimeCapabilitiesResponse,
  SkillPackageRecord,
  SkillRecord,
  SystemAgentRecord,
} from '../types';

import {
  buildApiUrl as buildSharedApiUrl,
  createBrowserAuthTokenStore,
  createJsonApiClient,
  normalizeApiBaseUrl,
  withQuery,
  type QueryValue,
} from '@chatos/frontend-runtime';

const RAW_API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || '').trim();
const API_BASE_URL = normalizeApiBaseUrl(RAW_API_BASE_URL);
const AUTH_TOKEN_STORAGE_KEY = 'plugin_management_service_auth_token';
const authTokenStore = createBrowserAuthTokenStore({
  storageKey: AUTH_TOKEN_STORAGE_KEY,
  changeEvent: 'plugin-management-auth-changed',
});

export function getAuthToken(): string | null {
  return authTokenStore.getAuthToken();
}

export function setAuthToken(token: string): void {
  authTokenStore.setAuthToken(token);
}

export function clearAuthToken(): void {
  authTokenStore.clearAuthToken();
}

function buildApiUrl(path: string): string {
  return buildSharedApiUrl(API_BASE_URL, path);
}

const request = createJsonApiClient({
  baseUrl: API_BASE_URL,
  getAuthToken,
  onUnauthorized: clearAuthToken,
});

export const api = {
  login: (payload: LoginPayload) =>
    request<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  currentUser: () => request<CurrentUser>('/api/auth/me'),
  listMcps: (params?: Record<string, QueryValue>) =>
    request<ListResponse<McpRecord>>(withQuery('/api/mcps', params || {})),
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
  listSkills: (params?: Record<string, QueryValue>) =>
    request<ListResponse<SkillRecord>>(withQuery('/api/skills', params || {})),
  createSkill: (payload: unknown) =>
    request<SkillRecord>('/api/skills', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateSkill: (id: string, payload: unknown) =>
    request<SkillRecord>(`/api/skills/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteSkill: (id: string) =>
    request<void>(`/api/skills/${id}`, {
      method: 'DELETE',
    }),
  checkSkill: (id: string) =>
    request<ResourceCheckRecord>(`/api/skills/${id}/check`, {
      method: 'POST',
    }),
  listSkillPackages: (params?: Record<string, QueryValue>) =>
    request<ListResponse<SkillPackageRecord>>(withQuery('/api/skill-packages', params || {})),
  createSkillPackage: (payload: unknown) =>
    request<SkillPackageRecord>('/api/skill-packages', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateSkillPackage: (id: string, payload: unknown) =>
    request<SkillPackageRecord>(`/api/skill-packages/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteSkillPackage: (id: string) =>
    request<void>(`/api/skill-packages/${id}`, {
      method: 'DELETE',
    }),
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
    request<AgentMcpBindingsResponse>(`/api/system-agents/${agentKey}/mcp-bindings`),
  updateAgentMcpBindings: (
    agentKey: string,
    bindings: Array<{ mcp_id: string; mode: string }>,
  ) =>
    request<AgentMcpBindingsResponse>(`/api/system-agents/${agentKey}/mcp-bindings`, {
      method: 'PUT',
      body: JSON.stringify({ bindings }),
    }),
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
    vendor: string,
    content: string,
    expectedUpdatedAt?: string,
  ) =>
    request<AgentProviderPromptRecord>(
      `/api/system-agents/${encodeURIComponent(agentKey)}/provider-prompts/${encodeURIComponent(vendor)}/draft`,
      {
        method: 'PUT',
        body: JSON.stringify({ content, expected_updated_at: expectedUpdatedAt }),
      },
    ),
  publishAgentProviderPrompt: (agentKey: string, vendor: string) =>
    request<AgentProviderPromptRecord>(
      `/api/system-agents/${encodeURIComponent(agentKey)}/provider-prompts/${encodeURIComponent(vendor)}/publish`,
      { method: 'POST', body: JSON.stringify({}) },
    ),
  generateAgentProviderPrompt: (
    agentKey: string,
    vendor: string,
    payload: { model_config_id: string; requirement: string; current_content: string },
  ) => request<GenerateAgentPromptResponse>(
    `/api/system-agents/${encodeURIComponent(agentKey)}/provider-prompts/${encodeURIComponent(vendor)}/generate`,
    { method: 'POST', body: JSON.stringify(payload) },
  ),
  agentPromptCompleteness: () =>
    request<AgentPromptCompleteness[]>('/api/system-agents/prompt-completeness'),
  resolveAgentCapabilities: (params: Record<string, QueryValue>) =>
    request<RuntimeCapabilitiesResponse>(withQuery('/api/runtime/agent-capabilities', params)),
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
