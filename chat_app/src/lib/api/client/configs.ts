import { debugLog } from '@/lib/utils';

import { buildQuery } from './shared';
import type { ApiRequestFn } from './workspace';

export const getMcpConfigs = (request: ApiRequestFn, userId?: string) => {
  const query = buildQuery({ user_id: userId });
  debugLog('🔍 getMcpConfigs API调用:', { userId, query });
  return request(`/mcp-configs${query}`);
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
) => {
  debugLog('🔍 API client createMcpConfig 调用:', data);
  return request('/mcp-configs', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateMcpConfig = (
  request: ApiRequestFn,
  id: string,
  data: {
    id?: string;
    name?: string;
    command?: string;
    type?: 'http' | 'stdio';
    args?: string[] | null;
    env?: Record<string, string> | null;
    cwd?: string | null;
    enabled?: boolean;
    userId?: string;
  }
) => {
  debugLog('🔍 API client updateMcpConfig 调用:', { id, data });
  return request(`/mcp-configs/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteMcpConfig = (request: ApiRequestFn, id: string) => {
  return request(`/mcp-configs/${id}`, {
    method: 'DELETE',
  });
};

export const getBuiltinMcpSettings = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/mcp-configs/${id}/builtin/settings`);
};

export const getBuiltinMcpPermissions = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/mcp-configs/${id}/builtin/mcp-permissions`);
};

export const updateBuiltinMcpPermissions = (
  request: ApiRequestFn,
  id: string,
  payload: { enabled_mcp_ids: string[]; selected_system_context_id?: string }
): Promise<any> => {
  return request<any>(`/mcp-configs/${id}/builtin/mcp-permissions`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const importBuiltinMcpAgents = (
  request: ApiRequestFn,
  id: string,
  content: string
): Promise<any> => {
  return request<any>(`/mcp-configs/${id}/builtin/import-agents`, {
    method: 'POST',
    body: JSON.stringify({ content }),
  });
};

export const importBuiltinMcpSkills = (
  request: ApiRequestFn,
  id: string,
  content: string
): Promise<any> => {
  return request<any>(`/mcp-configs/${id}/builtin/import-skills`, {
    method: 'POST',
    body: JSON.stringify({ content }),
  });
};

export const importBuiltinMcpFromGit = (
  request: ApiRequestFn,
  id: string,
  payload: { repository: string; branch?: string; agents_path?: string; skills_path?: string }
): Promise<any> => {
  return request<any>(`/mcp-configs/${id}/builtin/import-git`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const installBuiltinMcpPlugin = (
  request: ApiRequestFn,
  id: string,
  payload: { source?: string; install_all?: boolean }
): Promise<any> => {
  return request<any>(`/mcp-configs/${id}/builtin/install-plugin`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });
};

export const getAiModelConfigs = (request: ApiRequestFn, userId?: string) => {
  const query = buildQuery({ user_id: userId });
  debugLog('🔍 getAiModelConfigs API调用:', { userId, query });
  return request(`/ai-model-configs${query}`);
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
    user_id?: string;
    enabled: boolean;
    supports_images?: boolean;
    supports_reasoning?: boolean;
    supports_responses?: boolean;
  }
) => {
  return request('/ai-model-configs', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateAiModelConfig = (request: ApiRequestFn, id: string, data: any) => {
  return request(`/ai-model-configs/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteAiModelConfig = (request: ApiRequestFn, id: string) => {
  return request(`/ai-model-configs/${id}`, {
    method: 'DELETE',
  });
};

export const getSystemContexts = (request: ApiRequestFn, userId: string): Promise<any[]> => {
  return request<any[]>(`/system-contexts?user_id=${userId}`);
};

export const getActiveSystemContext = (
  request: ApiRequestFn,
  userId: string
): Promise<{ content: string; context: any }> => {
  return request<{ content: string; context: any }>(`/system-context/active?user_id=${userId}`);
};

export const createSystemContext = (
  request: ApiRequestFn,
  data: {
    name: string;
    content: string;
    user_id: string;
    app_ids?: string[];
  }
): Promise<any> => {
  debugLog('🔍 API client createSystemContext 调用:', data);
  debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
  return request<any>('/system-contexts', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateSystemContext = (
  request: ApiRequestFn,
  id: string,
  data: {
    name: string;
    content: string;
    app_ids?: string[];
  }
): Promise<any> => {
  debugLog('🔍 API client updateSystemContext 调用:', { id, data });
  debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
  return request<any>(`/system-contexts/${id}`, {
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
): Promise<any> => {
  return request<any>(`/system-contexts/${id}/activate`, {
    method: 'POST',
    body: JSON.stringify({ user_id: userId, is_active: true }),
  });
};

export const generateSystemContextDraft = (
  request: ApiRequestFn,
  data: {
    user_id: string;
    scene: string;
    style?: string;
    language?: string;
    output_format?: string;
    constraints?: string[];
    forbidden?: string[];
    candidate_count?: number;
    ai_model_config?: any;
  }
): Promise<any> => {
  return request<any>('/system-contexts/ai/generate', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const optimizeSystemContextDraft = (
  request: ApiRequestFn,
  data: {
    user_id: string;
    content: string;
    goal?: string;
    keep_intent?: boolean;
    ai_model_config?: any;
  }
): Promise<any> => {
  return request<any>('/system-contexts/ai/optimize', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const evaluateSystemContextDraft = (
  request: ApiRequestFn,
  data: { content: string }
): Promise<any> => {
  return request<any>('/system-contexts/ai/evaluate', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const getApplications = (request: ApiRequestFn, userId?: string): Promise<any[]> => {
  const query = buildQuery({ user_id: userId });
  return request<any[]>(`/applications${query}`);
};

export const getApplication = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/applications/${id}`);
};

export const createApplication = (
  request: ApiRequestFn,
  data: {
    name: string;
    url: string;
    icon_url?: string | null;
    user_id?: string;
  }
): Promise<any> => {
  return request<any>('/applications', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateApplication = (
  request: ApiRequestFn,
  id: string,
  data: { name?: string; url?: string; icon_url?: string | null }
): Promise<any> => {
  return request<any>(`/applications/${id}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteApplication = (request: ApiRequestFn, id: string): Promise<any> => {
  return request<any>(`/applications/${id}`, {
    method: 'DELETE',
  });
};

export const getAgents = (request: ApiRequestFn, userId?: string): Promise<any[]> => {
  const query = buildQuery({ user_id: userId });
  return request<any[]>(`/agents${query}`);
};

export const createAgent = (
  request: ApiRequestFn,
  data: {
    name: string;
    description?: string;
    ai_model_config_id: string;
    mcp_config_ids?: string[];
    callable_agent_ids?: string[];
    system_context_id?: string;
    project_id?: string | null;
    workspace_dir?: string | null;
    user_id?: string;
    enabled?: boolean;
    app_ids?: string[];
  }
): Promise<any> => {
  debugLog('🔍 API client createAgent 调用:', data);
  debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
  return request<any>('/agents', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateAgent = (
  request: ApiRequestFn,
  agentId: string,
  data: {
    name?: string;
    description?: string;
    ai_model_config_id?: string;
    mcp_config_ids?: string[];
    callable_agent_ids?: string[];
    system_context_id?: string;
    project_id?: string | null;
    workspace_dir?: string | null;
    enabled?: boolean;
    app_ids?: string[];
  }
): Promise<any> => {
  debugLog('🔍 API client updateAgent 调用:', { agentId, data });
  debugLog('🔍 [关键] app_ids 字段:', data.app_ids, '类型:', typeof data.app_ids, '是否为数组:', Array.isArray(data.app_ids));
  return request<any>(`/agents/${agentId}`, {
    method: 'PUT',
    body: JSON.stringify(data),
  });
};

export const deleteAgent = (request: ApiRequestFn, agentId: string): Promise<any> => {
  return request<any>(`/agents/${agentId}`, {
    method: 'DELETE',
  });
};
