import type {
  AuditEvent,
  ConfigDefinition,
  ConfigDraft,
  ConfigRelease,
  ConfigValue,
  DraftResponse,
  EffectiveConfig,
  QueueOperationsResponse,
  QueueReplayRequest,
  QueueReplayResponse,
  ServiceInstance,
} from './types';

import {
  createJsonApiClient,
} from '@chatos/frontend-runtime';
import {
  clearAuthToken as clearSharedAuthToken,
  getAuthToken as getSharedAuthToken,
} from '../../shared/auth/tokenStore';
import { ADMIN_SERVICE_BASES, stripBackendApiPrefix } from '../../shared/api/servicePaths';

export function getToken(): string | null {
  return getSharedAuthToken();
}

export function clearToken(): void {
  clearSharedAuthToken();
}

const rawRequest = createJsonApiClient({
  baseUrl: ADMIN_SERVICE_BASES.configCenter,
  getAuthToken: getToken,
  onUnauthorized: clearToken,
  readSuccessResponse: (response) => response.json(),
  overrideContentType: true,
});

const request = <T,>(path: string, init?: RequestInit) =>
  rawRequest<T>(stripBackendApiPrefix(path), init);

export const api = {
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
  queueOperations: (environment: string) =>
    request<QueueOperationsResponse>(
      `/api/config/v1/environments/${encodeURIComponent(environment)}/queue-operations`,
    ),
  replayQueueItem: (
    environment: string,
    payload: QueueReplayRequest,
  ) =>
    request<QueueReplayResponse>(
      `/api/config/v1/environments/${encodeURIComponent(environment)}/queue-operations`,
      { method: 'POST', body: JSON.stringify(payload) },
    ),
  instances: () => request<ServiceInstance[]>('/api/config/v1/instances'),
};
