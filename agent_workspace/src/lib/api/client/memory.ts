import { buildQuery } from './shared';
import type {
  MemoryAgentResponse,
  MemoryAgentRuntimeContextResponse,
  MemoryAgentsQueryOptions,
} from './types';
import type { ApiRequestFn } from './workspace';

export const getMemoryAgents = (
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
  return request<MemoryAgentResponse[]>(`/memory-agents${query}`);
};

export const getMemoryAgentRuntimeContext = (
  request: ApiRequestFn,
  agentId: string,
): Promise<MemoryAgentRuntimeContextResponse> => {
  return request<MemoryAgentRuntimeContextResponse>(`/memory-agents/${encodeURIComponent(agentId)}/runtime-context`);
};
