// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  AgentAccountListItem,
  CreateAgentAccountPayload,
  CreateInviteCodePayload,
  CreateInviteCodeResponse,
  CreateUserPayload,
  CurrentUserResponse,
  HealthResponse,
  LoginPayload,
  LoginResponse,
  ResetAgentPasswordPayload,
  SystemConfigResponse,
  CreateUserModelConfigPayload,
  CreateUserModelProviderPayload,
  ProvisionHarnessPayload,
  UpdateUserModelConfigPayload,
  UpdateUserModelProviderPayload,
  UpdateUserModelSettingsPayload,
  UpdateAgentAccountPayload,
  UpdateUserPayload,
  UserModelConfigRecord,
  UserModelProviderRecord,
  UserModelSettingsRecord,
  UserSummaryRecord,
  InviteCodeRecord,
} from '../types';

import {
  buildApiUrl as buildSharedApiUrl,
  createBrowserAuthTokenStore,
  createJsonApiClient,
  normalizeApiBaseUrl,
} from '@chatos/frontend-runtime';

const RAW_API_BASE_URL = (import.meta.env.VITE_API_BASE_URL || '').trim();
const API_BASE_URL = normalizeApiBaseUrl(RAW_API_BASE_URL || resolveDefaultApiBaseUrl(), {
  stripApiSuffix: false,
});
const AUTH_TOKEN_STORAGE_KEY = 'user_service_auth_token';
const authTokenStore = createBrowserAuthTokenStore({
  storageKey: AUTH_TOKEN_STORAGE_KEY,
  changeEvent: 'user-service-auth-changed',
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

export function buildApiUrl(path: string): string {
  return buildSharedApiUrl(API_BASE_URL, path);
}

function resolveDefaultApiBaseUrl(): string {
  const baseUrl = (import.meta.env.BASE_URL || '').trim();
  if (!baseUrl || baseUrl === '/') {
    return '';
  }
  return baseUrl.replace(/\/+$/, '');
}

const request = createJsonApiClient({
  baseUrl: API_BASE_URL,
  getAuthToken,
  onUnauthorized: clearAuthToken,
  readErrorMessage: async (response) => {
    let message = response.statusText;
    try {
      const data = (await response.json()) as { error?: string; detail?: string };
      if (data.error || data.detail) {
        message = [data.error, data.detail].filter(Boolean).join(': ');
      }
    } catch {
      // Keep the HTTP status text for non-JSON error bodies.
    }
    return message;
  },
  readSuccessResponse: (response) => response.json(),
});

export const api = {
  health: () => request<HealthResponse>('/api/health'),
  login: (payload: LoginPayload) =>
    request<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  currentUser: () => request<CurrentUserResponse>('/api/auth/me'),
  logout: () =>
    request<void>('/api/auth/logout', {
      method: 'POST',
    }),
  getSystemConfig: () => request<SystemConfigResponse>('/api/system/config'),
  listUsers: () => request<UserSummaryRecord[]>('/api/users'),
  listInviteCodes: () => request<InviteCodeRecord[]>('/api/invite-codes'),
  createInviteCode: (payload: CreateInviteCodePayload) =>
    request<CreateInviteCodeResponse>('/api/invite-codes', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  revokeInviteCode: (id: string) =>
    request<InviteCodeRecord>(`/api/invite-codes/${id}/revoke`, {
      method: 'POST',
    }),
  createUser: (payload: CreateUserPayload) =>
    request<UserSummaryRecord>('/api/users', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateUser: (id: string, payload: UpdateUserPayload) =>
    request<UserSummaryRecord>(`/api/users/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  provisionHarnessUser: (id: string, payload: ProvisionHarnessPayload) =>
    request<UserSummaryRecord>(`/api/users/${id}/harness-provisioning`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  retryHarnessProvisioning: (id: string) =>
    request<UserSummaryRecord>(`/api/users/${id}/harness-provisioning/retry`, {
      method: 'POST',
    }),
  listAgentAccounts: () => request<AgentAccountListItem[]>('/api/agent-accounts'),
  createAgentAccount: (payload: CreateAgentAccountPayload) =>
    request<AgentAccountListItem>('/api/agent-accounts', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateAgentAccount: (id: string, payload: UpdateAgentAccountPayload) =>
    request<AgentAccountListItem>(`/api/agent-accounts/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  listModelConfigs: (userId?: string) =>
    request<UserModelConfigRecord[]>(
      `/api/model-configs${userId ? `?user_id=${encodeURIComponent(userId)}` : ''}`,
    ),
  listModelProviders: (userId?: string) =>
    request<UserModelProviderRecord[]>(
      `/api/model-providers${userId ? `?user_id=${encodeURIComponent(userId)}` : ''}`,
    ),
  getModelProvider: (id: string, includeSecret?: boolean) =>
    request<UserModelProviderRecord>(
      `/api/model-providers/${id}${includeSecret ? '?include_secret=true' : ''}`,
    ),
  createModelProvider: (payload: CreateUserModelProviderPayload) =>
    request<UserModelProviderRecord>('/api/model-providers', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateModelProvider: (id: string, payload: UpdateUserModelProviderPayload) =>
    request<UserModelProviderRecord>(`/api/model-providers/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  refreshModelProvider: (id: string, payload: UpdateUserModelProviderPayload) =>
    request<UserModelProviderRecord>(`/api/model-providers/${id}/refresh`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  deleteModelProvider: (id: string) =>
    request<void>(`/api/model-providers/${id}`, {
      method: 'DELETE',
    }),
  createModelConfig: (payload: CreateUserModelConfigPayload) =>
    request<UserModelConfigRecord>('/api/model-configs', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  updateModelConfig: (id: string, payload: UpdateUserModelConfigPayload) =>
    request<UserModelConfigRecord>(`/api/model-configs/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    }),
  deleteModelConfig: (id: string) =>
    request<void>(`/api/model-configs/${id}`, {
      method: 'DELETE',
    }),
  getModelSettings: (userId: string) =>
    request<UserModelSettingsRecord>(`/api/model-configs/settings?user_id=${encodeURIComponent(userId)}`),
  updateModelSettings: (payload: UpdateUserModelSettingsPayload) =>
    request<UserModelSettingsRecord>('/api/model-configs/settings', {
      method: 'PUT',
      body: JSON.stringify(payload),
    }),
  resetAgentPassword: (id: string, payload: ResetAgentPasswordPayload) =>
    request<void>(`/api/agent-accounts/${id}/reset-password`, {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
};
