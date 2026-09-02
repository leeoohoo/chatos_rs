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
  createJsonApiClient,
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

const API_BASE_URL = ADMIN_SERVICE_BASES.userService;

export function getAuthToken(): string | null {
  return getSharedAuthToken();
}

export function clearAuthToken(): void {
  clearSharedAuthToken();
}

export function buildApiUrl(path: string): string {
  return buildAdminServiceUrl(API_BASE_URL, path);
}

const rawRequest = createJsonApiClient({
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

const request = <T,>(path: string, init?: RequestInit) =>
  rawRequest<T>(stripBackendApiPrefix(path), init);

export const api = {
  health: () => request<HealthResponse>('/api/health'),
  currentUser: () => request<CurrentUserResponse>('/api/auth/me'),
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
