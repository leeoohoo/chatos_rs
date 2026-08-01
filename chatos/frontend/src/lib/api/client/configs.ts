// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { debugLog } from '@/lib/utils';

import { buildQuery } from './shared';
import type {
  ActiveSystemContextResponse,
  AiModelConfigCreatePayload,
  AiModelProviderCreatePayload,
  AiModelProviderResponse,
  AiModelProviderUpdatePayload,
  AiModelSettingsResponse,
  AiModelSettingsUpdatePayload,
  AiProviderModelsResponse,
  AiModelConfigResponse,
  AiModelConfigUpdatePayload,
  ApplicationResponse,
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

export const getAiModelConfigs = (request: ApiRequestFn): Promise<AiModelConfigResponse[]> => {
  debugLog('🔍 getAiModelConfigs API调用');
  return request<AiModelConfigResponse[]>('/ai-model-configs');
};

export const getAiModelConfig = (
  request: ApiRequestFn,
  id: string,
  options?: { includeSecret?: boolean },
): Promise<AiModelConfigResponse> => {
  const query = buildQuery({ include_secret: options?.includeSecret ? 'true' : undefined });
  return request<AiModelConfigResponse>(`/ai-model-configs/${id}${query}`);
};

export const getAiModelProviders = (request: ApiRequestFn): Promise<AiModelProviderResponse[]> => {
  return request<AiModelProviderResponse[]>('/ai-model-providers');
};

export const getAiModelProvider = (
  request: ApiRequestFn,
  id: string,
  options?: { includeSecret?: boolean },
): Promise<AiModelProviderResponse> => {
  const query = buildQuery({ include_secret: options?.includeSecret ? 'true' : undefined });
  return request<AiModelProviderResponse>(`/ai-model-providers/${id}${query}`);
};

export const createAiModelProvider = (
  request: ApiRequestFn,
  data: AiModelProviderCreatePayload,
): Promise<AiModelProviderResponse> => {
  return request<AiModelProviderResponse>('/ai-model-providers', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateAiModelProvider = (
  request: ApiRequestFn,
  id: string,
  data: AiModelProviderUpdatePayload,
): Promise<AiModelProviderResponse> => {
  return request<AiModelProviderResponse>(`/ai-model-providers/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const refreshAiModelProvider = (
  request: ApiRequestFn,
  id: string,
  data: AiModelProviderUpdatePayload,
): Promise<AiModelProviderResponse> => {
  return request<AiModelProviderResponse>(`/ai-model-providers/${id}/refresh`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const deleteAiModelProvider = (request: ApiRequestFn, id: string): Promise<{ success?: boolean }> => {
  return request<{ success?: boolean }>(`/ai-model-providers/${id}`, {
    method: 'DELETE',
  });
};

export const createAiModelConfig = (
  request: ApiRequestFn,
  data: AiModelConfigCreatePayload,
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

export const refreshAiModelConfig = (
  request: ApiRequestFn,
  id: string,
  data: AiModelConfigUpdatePayload,
): Promise<AiModelConfigResponse> => {
  return request<AiModelConfigResponse>(`/ai-model-configs/${id}/refresh`, {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const deleteAiModelConfig = (request: ApiRequestFn, id: string): Promise<{ success?: boolean }> => {
  return request<{ success?: boolean }>(`/ai-model-configs/${id}`, {
    method: 'DELETE',
  });
};

export const getAiProviderModels = (
  request: ApiRequestFn,
  id: string,
  options?: { refresh?: boolean },
): Promise<AiProviderModelsResponse> => {
  const query = buildQuery({ refresh: options?.refresh ? 'true' : undefined });
  return request<AiProviderModelsResponse>(`/ai-model-configs/${id}/models${query}`);
};

export const getAiModelSettings = (
  request: ApiRequestFn,
): Promise<AiModelSettingsResponse> => {
  return request<AiModelSettingsResponse>('/ai-model-settings');
};

export const updateAiModelSettings = (
  request: ApiRequestFn,
  data: AiModelSettingsUpdatePayload,
): Promise<AiModelSettingsResponse> => {
  return request<AiModelSettingsResponse>('/ai-model-settings', {
    method: 'PUT',
    body: JSON.stringify(data),
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
