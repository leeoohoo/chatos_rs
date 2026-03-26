import { buildQuery } from './shared';
import type { MemoryAgentsQueryOptions } from './types';
import type { ApiRequestFn } from './workspace';

export const getMemoryAgents = (
  request: ApiRequestFn,
  userId?: string,
  options?: MemoryAgentsQueryOptions,
): Promise<any[]> => {
  const query = buildQuery({
    user_id: userId,
    enabled: typeof options?.enabled === 'boolean' ? options.enabled : undefined,
    limit: options?.limit,
    offset: options?.offset,
  });
  return request<any[]>(`/memory-agents${query}`);
};

export const getMemoryAgentRuntimeContext = (
  request: ApiRequestFn,
  agentId: string,
): Promise<any> => {
  return request<any>(`/memory-agents/${encodeURIComponent(agentId)}/runtime-context`);
};
