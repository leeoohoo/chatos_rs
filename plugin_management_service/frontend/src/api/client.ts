// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
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

const RAW_API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || '').trim();
const API_BASE_URL = RAW_API_BASE_URL.replace(/\/+$/, '').replace(/\/api$/, '');
const AUTH_TOKEN_STORAGE_KEY = 'plugin_management_service_auth_token';

export function getAuthToken(): string | null {
  return window.localStorage.getItem(AUTH_TOKEN_STORAGE_KEY);
}

export function setAuthToken(token: string): void {
  window.localStorage.setItem(AUTH_TOKEN_STORAGE_KEY, token);
  window.dispatchEvent(new Event('plugin-management-auth-changed'));
}

export function clearAuthToken(): void {
  window.localStorage.removeItem(AUTH_TOKEN_STORAGE_KEY);
  window.dispatchEvent(new Event('plugin-management-auth-changed'));
}

function buildApiUrl(path: string): string {
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;
  return API_BASE_URL ? `${API_BASE_URL}${normalizedPath}` : normalizedPath;
}

type QueryValue = string | number | boolean | null | undefined;

function withQuery(path: string, params: Record<string, QueryValue>): string {
  const search = new URLSearchParams();
  Object.entries(params).forEach(([key, value]) => {
    if (value === undefined || value === null) {
      return;
    }
    const text = String(value).trim();
    if (text) {
      search.set(key, text);
    }
  });
  const query = search.toString();
  return query ? `${path}?${query}` : path;
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const headers = new Headers(init?.headers);
  if (!headers.has('Content-Type')) {
    headers.set('Content-Type', 'application/json');
  }
  const token = getAuthToken();
  if (token && !headers.has('Authorization')) {
    headers.set('Authorization', `Bearer ${token}`);
  }
  const response = await fetch(buildApiUrl(path), {
    ...init,
    headers,
  });
  if (!response.ok) {
    let message = response.statusText;
    try {
      const data = (await response.json()) as { error?: string };
      if (data.error) {
        message = data.error;
      }
    } catch {
      // keep status text
    }
    if (response.status === 401) {
      clearAuthToken();
    }
    throw new Error(message);
  }
  if (response.status === 204) {
    return undefined as T;
  }
  const text = await response.text();
  if (!text.trim()) {
    return undefined as T;
  }
  return JSON.parse(text) as T;
}

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
