import type {
  AuditEvent,
  ConfigDefinition,
  ConfigDraft,
  ConfigRelease,
  ConfigValue,
  CurrentUser,
  DraftResponse,
  EffectiveConfig,
  LoginResponse,
  ServiceInstance,
} from './types';

import {
  createBrowserAuthTokenStore,
  createJsonApiClient,
} from '@chatos/frontend-runtime';

const TOKEN_KEY = 'chatos.configuration-center.token';
const authTokenStore = createBrowserAuthTokenStore({ storageKey: TOKEN_KEY });

export function getToken(): string | null {
  return authTokenStore.getAuthToken();
}

export function setToken(token: string): void {
  authTokenStore.setAuthToken(token);
}

export function clearToken(): void {
  authTokenStore.clearAuthToken();
}

const request = createJsonApiClient({
  getAuthToken: getToken,
  onUnauthorized: clearToken,
  readSuccessResponse: (response) => response.json(),
  overrideContentType: true,
});

export const api = {
  login: (username: string, password: string) =>
    request<LoginResponse>('/api/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  me: () => request<CurrentUser>('/api/auth/me'),
  catalog: () => request<ConfigDefinition[]>('/api/config/v1/catalog'),
  createCustomDefinition: (payload: {
    environment: string;
    key: string;
    display_name: string;
    description?: string;
    category?: string;
    scope: string;
    service_name?: string;
    value_type: string;
    default_value: ConfigValue;
    reload_mode: string;
    env_aliases: string[];
  }) =>
    request<ConfigDefinition>('/api/config/v1/catalog/custom', {
      method: 'POST',
      body: JSON.stringify(payload),
    }),
  effective: (environment: string) =>
    request<EffectiveConfig>(`/api/config/v1/environments/${encodeURIComponent(environment)}/effective`),
  draft: (environment: string) =>
    request<DraftResponse>(`/api/config/v1/environments/${encodeURIComponent(environment)}/draft`),
  saveDraft: (environment: string, changes: Record<string, ConfigValue>) =>
    request<ConfigDraft>(`/api/config/v1/environments/${encodeURIComponent(environment)}/draft`, {
      method: 'PUT',
      body: JSON.stringify({ changes }),
    }),
  validateDraft: (environment: string) =>
    request<{ valid: boolean; errors: string[] }>(
      `/api/config/v1/environments/${encodeURIComponent(environment)}/draft/validate`,
      { method: 'POST', body: '{}' },
    ),
  publishDraft: (environment: string, message: string) =>
    request<ConfigRelease>(
      `/api/config/v1/environments/${encodeURIComponent(environment)}/draft/publish`,
      { method: 'POST', body: JSON.stringify({ message }) },
    ),
  releases: (environment: string) =>
    request<ConfigRelease[]>(
      `/api/config/v1/environments/${encodeURIComponent(environment)}/releases?limit=200`,
    ),
  rollback: (environment: string, releaseId: string) =>
    request<ConfigRelease>(
      `/api/config/v1/environments/${encodeURIComponent(environment)}/releases/${encodeURIComponent(releaseId)}/rollback`,
      { method: 'POST', body: '{}' },
    ),
  audit: () => request<AuditEvent[]>('/api/config/v1/audit-events?limit=300'),
  instances: () => request<ServiceInstance[]>('/api/config/v1/instances'),
};
