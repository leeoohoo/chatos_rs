import { debugLog } from '@/lib/utils';

import { buildQuery } from './shared';
import type {
  ActiveSystemContextResponse,
  AiModelConfigResponse,
  AiModelConfigUpdatePayload,
  ApplicationResponse,
  McpConfigResponse,
  McpConfigUpdatePayload,
  SystemContextCreatePayload,
  SystemContextDraftEvaluatePayload,
  SystemContextDraftEvaluateResponse,
  SystemContextDraftGeneratePayload,
  SystemContextDraftGenerateResponse,
  SystemContextDraftOptimizePayload,
  SystemContextDraftOptimizeResponse,
  SystemContextResponse,
  SystemContextUpdatePayload,
} from './types';
import type { ApiRequestFn } from './workspace';

interface McpConfigCacheState {
  data: McpConfigResponse[] | null;
  inflight: Promise<McpConfigResponse[]> | null;
}

const mcpConfigCaches = new WeakMap<ApiRequestFn, McpConfigCacheState>();

const getOrCreateMcpConfigCacheState = (request: ApiRequestFn): McpConfigCacheState => {
  const existing = mcpConfigCaches.get(request);
  if (existing) {
    return existing;
  }
  const next: McpConfigCacheState = {
    data: null,
    inflight: null,
  };
  mcpConfigCaches.set(request, next);
  return next;
};

export const invalidateMcpConfigCache = (request: ApiRequestFn): void => {
  const cacheState = getOrCreateMcpConfigCacheState(request);
  cacheState.data = null;
  cacheState.inflight = null;
};

export const getMcpConfigs = (
  request: ApiRequestFn,
  userId?: string,
  options?: { forceRefresh?: boolean },
): Promise<McpConfigResponse[]> => {
  const query = buildQuery({ user_id: userId });
  debugLog('🔍 getMcpConfigs API调用:', { userId, query });
  const cacheState = getOrCreateMcpConfigCacheState(request);
  if (!options?.forceRefresh && Array.isArray(cacheState.data)) {
    return Promise.resolve([...cacheState.data]);
  }
  if (!options?.forceRefresh && cacheState.inflight) {
    return cacheState.inflight.then((rows) => [...rows]);
  }

  const inflight = request<McpConfigResponse[]>(`/mcp-configs${query}`)
    .then((rows) => {
      const normalized = Array.isArray(rows) ? rows : [];
      cacheState.data = normalized;
      return normalized;
    })
    .finally(() => {
      if (cacheState.inflight === inflight) {
        cacheState.inflight = null;
      }
    });

  cacheState.inflight = inflight;
  return inflight.then((rows) => [...rows]);
};

export const createMcpConfig = (
  request: ApiRequestFn,
  data: {
    id: string;
    name: string;
    command: string;
    type: 'http' | 'stdio';
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    enabled: boolean;
    user_id?: string;
  }
): Promise<McpConfigResponse> => {
  debugLog('🔍 API client createMcpConfig 调用:', data);
  return request<McpConfigResponse>('/mcp-configs', {
    method: 'POST',
    body: JSON.stringify(data),
  }).then((result) => {
    invalidateMcpConfigCache(request);
    return result;
  });
};

export const updateMcpConfig = (
  request: ApiRequestFn,
  id: string,
  data: McpConfigUpdatePayload,
): Promise<McpConfigResponse> => {
  debugLog('🔍 API client updateMcpConfig 调用:', { id, data });
  return request<McpConfigResponse>(`/mcp-configs/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  }).then((result) => {
    invalidateMcpConfigCache(request);
    return result;
  });
};

export const deleteMcpConfig = (request: ApiRequestFn, id: string): Promise<{ success?: boolean }> => {
  return request<{ success?: boolean }>(`/mcp-configs/${id}`, {
    method: 'DELETE',
  }).then((result) => {
    invalidateMcpConfigCache(request);
    return result;
  });
};

export const getAiModelConfigs = (request: ApiRequestFn): Promise<AiModelConfigResponse[]> => {
  debugLog('🔍 getAiModelConfigs API调用');
  return request<AiModelConfigResponse[]>('/ai-model-configs');
};

export const createAiModelConfig = (
  request: ApiRequestFn,
  data: {
    id: string;
    name: string;
    provider: string;
    model: string;
    thinking_level?: string;
    api_key: string;
    base_url: string;
    enabled: boolean;
    supports_images?: boolean;
    supports_reasoning?: boolean;
    supports_responses?: boolean;
  }
): Promise<AiModelConfigResponse> => {
  return request<AiModelConfigResponse>('/ai-model-configs', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateAiModelConfig = (
  request: ApiRequestFn,
  id: string,
  data: AiModelConfigUpdatePayload,
): Promise<AiModelConfigResponse> => {
  return request<AiModelConfigResponse>(`/ai-model-configs/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteAiModelConfig = (request: ApiRequestFn, id: string): Promise<{ success?: boolean }> => {
  return request<{ success?: boolean }>(`/ai-model-configs/${id}`, {
    method: 'DELETE',
  });
};

export const getSystemContexts = (request: ApiRequestFn, userId: string): Promise<SystemContextResponse[]> => {
  return request<SystemContextResponse[]>(`/system-contexts?user_id=${userId}`);
};

export const getActiveSystemContext = (
  request: ApiRequestFn,
  userId: string
): Promise<ActiveSystemContextResponse> => {
  return request<ActiveSystemContextResponse>(`/system-context/active?user_id=${userId}`);
};

export const createSystemContext = (
  request: ApiRequestFn,
  data: SystemContextCreatePayload,
): Promise<SystemContextResponse> => {
  debugLog('🔍 API client createSystemContext 调用:', data);
  debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
  return request<SystemContextResponse>('/system-contexts', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateSystemContext = (
  request: ApiRequestFn,
  id: string,
  data: SystemContextUpdatePayload,
): Promise<SystemContextResponse> => {
  debugLog('🔍 API client updateSystemContext 调用:', { id, data });
  debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
  return request<SystemContextResponse>(`/system-contexts/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteSystemContext = (request: ApiRequestFn, id: string): Promise<void> => {
  return request<void>(`/system-contexts/${id}`, {
    method: 'DELETE',
  });
};

export const activateSystemContext = (
  request: ApiRequestFn,
  id: string,
  userId: string
): Promise<SystemContextResponse> => {
  return request<SystemContextResponse>(`/system-contexts/${id}/activate`, {
    method: 'POST',
    body: JSON.stringify({ user_id: userId, is_active: true }),
  });
};

export const generateSystemContextDraft = (
  request: ApiRequestFn,
  data: SystemContextDraftGeneratePayload,
): Promise<SystemContextDraftGenerateResponse> => {
  return request<SystemContextDraftGenerateResponse>('/system-contexts/ai/generate', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const optimizeSystemContextDraft = (
  request: ApiRequestFn,
  data: SystemContextDraftOptimizePayload,
): Promise<SystemContextDraftOptimizeResponse> => {
  return request<SystemContextDraftOptimizeResponse>('/system-contexts/ai/optimize', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const evaluateSystemContextDraft = (
  request: ApiRequestFn,
  data: SystemContextDraftEvaluatePayload,
): Promise<SystemContextDraftEvaluateResponse> => {
  return request<SystemContextDraftEvaluateResponse>('/system-contexts/ai/evaluate', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getApplications = (request: ApiRequestFn, userId?: string): Promise<ApplicationResponse[]> => {
  const query = buildQuery({ user_id: userId });
  return request<ApplicationResponse[]>(`/applications${query}`);
};

export const getApplication = (request: ApiRequestFn, id: string): Promise<ApplicationResponse> => {
  return request<ApplicationResponse>(`/applications/${id}`);
};

export const createApplication = (
  request: ApiRequestFn,
  data: {
    name: string;
    url: string;
    icon_url?: string | null;
    user_id?: string;
  }
): Promise<ApplicationResponse> => {
  return request<ApplicationResponse>('/applications', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateApplication = (
  request: ApiRequestFn,
  id: string,
  data: { name?: string; url?: string; icon_url?: string | null }
): Promise<ApplicationResponse> => {
  return request<ApplicationResponse>(`/applications/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteApplication = (request: ApiRequestFn, id: string): Promise<{ success?: boolean }> => {
  return request<{ success?: boolean }>(`/applications/${id}`, {
    method: 'DELETE',
  });
};
