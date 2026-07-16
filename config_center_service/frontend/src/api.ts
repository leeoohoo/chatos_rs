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

const TOKEN_KEY = 'chatos.configuration-center.token';

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}

export function setToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
}

export function clearToken(): void {
  localStorage.removeItem(TOKEN_KEY);
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set('Content-Type', 'application/json');
  const token = getToken();
  if (token) {
    headers.set('Authorization', `Bearer ${token}`);
  }
  const response = await fetch(path, { ...init, headers });
  if (!response.ok) {
    if (response.status === 401) {
      clearToken();
    }
    let message = response.statusText;
    try {
      const payload = await response.json() as { error?: string };
      message = payload.error || message;
    } catch {
      // Ignore non-JSON error bodies.
    }
    throw new Error(message);
  }
  return response.json() as Promise<T>;
}

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
