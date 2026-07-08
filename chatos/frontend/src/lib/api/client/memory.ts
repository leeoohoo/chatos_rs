// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildQuery } from './shared';
import type {
  AiCreateAgentResponse,
  AiCreateAgentPayload,
  CreateAgentPayload,
  MemoryAgentResponse,
  MemoryAgentSessionResponse,
  MemoryAgentRuntimeContextResponse,
  MemoryAgentSessionsQueryOptions,
  MemoryAgentsQueryOptions,
  MemorySkillsQueryOptions,
  MemorySkillPluginResponse,
  MemorySkillResponse,
  UpdateAgentPayload,
} from './types';
import type { ApiRequestFn } from './workspace';

export const getMemoryAgents = (
  request: ApiRequestFn,
  userId?: string,
  options?: MemoryAgentsQueryOptions,
): Promise<MemoryAgentResponse[]> => {
  return getAgents(request, userId, options);
};

export const getAgents = (
  request: ApiRequestFn,
  userId?: string,
  options?: MemoryAgentsQueryOptions,
): Promise<MemoryAgentResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    enabled: typeof options?.enabled === 'boolean' ? options.enabled : undefined,
    limit: options?.limit,
    offset: options?.offset,
  });
  return request<MemoryAgentResponse[]>(`/agents${query}`);
};

export const getMemoryAgentRuntimeContext = (
  request: ApiRequestFn,
  agentId: string,
): Promise<MemoryAgentRuntimeContextResponse> => {
  return getAgentRuntimeContext(request, agentId);
};

export const getAgentRuntimeContext = (
  request: ApiRequestFn,
  agentId: string,
): Promise<MemoryAgentRuntimeContextResponse> => {
  return request<MemoryAgentRuntimeContextResponse>(`/agents/${encodeURIComponent(agentId)}/runtime-context`);
};

export const getAgentSessions = (
  request: ApiRequestFn,
  agentId: string,
  userId?: string,
  options?: MemoryAgentSessionsQueryOptions,
): Promise<MemoryAgentSessionResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    project_id: options?.project_id,
    status: options?.status,
    limit: options?.limit,
    offset: options?.offset,
  });
  return request<MemoryAgentSessionResponse[]>(`/agents/${encodeURIComponent(agentId)}/sessions${query}`);
};

export const createAgent = (
  request: ApiRequestFn,
  data: CreateAgentPayload,
): Promise<MemoryAgentResponse> => {
  return request<MemoryAgentResponse>('/agents', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const updateAgent = (
  request: ApiRequestFn,
  agentId: string,
  data: UpdateAgentPayload,
): Promise<MemoryAgentResponse> => {
  return request<MemoryAgentResponse>(`/agents/${encodeURIComponent(agentId)}`, {
    method: 'PATCH',
    body: JSON.stringify(data),
  });
};

export const deleteAgent = (
  request: ApiRequestFn,
  agentId: string,
): Promise<{ success?: boolean }> => {
  return request<{ success?: boolean }>(`/agents/${encodeURIComponent(agentId)}`, {
    method: 'DELETE',
  });
};

export const aiCreateAgent = (
  request: ApiRequestFn,
  data: AiCreateAgentPayload,
): Promise<AiCreateAgentResponse> => {
  return request<AiCreateAgentResponse>('/agents/ai-create', {
    method: 'POST',
    body: JSON.stringify(data),
  });
};

export const listSkillPlugins = (
  request: ApiRequestFn,
  userId?: string,
  options?: { limit?: number; offset?: number },
): Promise<MemorySkillPluginResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    limit: options?.limit,
    offset: options?.offset,
  });
  return request<MemorySkillPluginResponse[]>(`/skills/plugins${query}`);
};

export const listSkills = (
  request: ApiRequestFn,
  userId?: string,
  options?: MemorySkillsQueryOptions,
): Promise<MemorySkillResponse[]> => {
  const query = buildQuery({
    user_id: userId,
    plugin_source: options?.plugin_source,
    query: options?.query,
    limit: options?.limit,
    offset: options?.offset,
  });
  return request<MemorySkillResponse[]>(`/skills${query}`);
};

export const getSkill = (
  request: ApiRequestFn,
  skillId: string,
): Promise<MemorySkillResponse> => {
  return request<MemorySkillResponse>(`/skills/${encodeURIComponent(skillId)}`);
};

export const getSkillPlugin = (
  request: ApiRequestFn,
  source: string,
): Promise<MemorySkillPluginResponse> => {
  return request<MemorySkillPluginResponse>(`/skills/plugins/detail?source=${encodeURIComponent(source)}`);
};
