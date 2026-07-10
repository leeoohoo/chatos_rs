// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentMcpBindingsResponse,
  CurrentUser,
  ListResponse,
  LoginPayload,
  LoginResponse,
  McpRecord,
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
