import type ApiClient from '../../api/client';
import { normalizeAgent } from '../../domain/configs';
import { mergeSessionAiSelectionIntoMetadata } from '../helpers/sessionAiSelection';
import { mergeSessionRuntimeIntoMetadata } from '../helpers/sessionRuntime';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';
import { debugLog } from '@/lib/utils';
import type { AgentConfig } from '../../../types';
import { generateId } from '@/lib/utils';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}

interface LoadAgentsOptions {
  force?: boolean;
}

interface AiCreateAgentActionPayload {
  model_config_id?: string;
  requirement: string;
  name?: string;
  category?: string;
  description?: string;
  role_definition?: string;
  skill_ids?: string[];
  skill_prompts?: string[];
  enabled?: boolean;
  mcp_enabled?: boolean;
  enabled_mcp_ids?: string[];
  project_id?: string;
  project_root?: string;
}

interface AgentsCacheEntry {
  agents: AgentConfig[];
  stale: boolean;
}

interface AgentsClientCacheState {
  cache: Map<string, AgentsCacheEntry>;
  inflight: Map<string, Promise<AgentConfig[]>>;
}

const agentsClientCaches = new WeakMap<ApiClient, AgentsClientCacheState>();

const normalizeUserId = (userId: string): string => String(userId || '').trim();

const getOrCreateClientCacheState = (apiClient: ApiClient): AgentsClientCacheState => {
  const existing = agentsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: AgentsClientCacheState = {
    cache: new Map(),
    inflight: new Map(),
  };
  agentsClientCaches.set(apiClient, next);
  return next;
};

export function createAgentActions({ set, get, client, getUserIdParam }: Deps) {
  const syncLoadedAgents = (userId: string, agents: AgentConfig[]) => {
    getOrCreateClientCacheState(client).cache.set(normalizeUserId(userId), {
      agents,
      stale: false,
    });
  };

  return {
    loadAgents: async (options?: LoadAgentsOptions) => {
      try {
        const userId = getUserIdParam();
        const cacheKey = normalizeUserId(userId);
        const cacheState = getOrCreateClientCacheState(client);
        const cached = cacheState.cache.get(cacheKey);
        if (!options?.force && cached && !cached.stale) {
          set((state: ChatStoreDraft) => {
            state.agents = cached.agents || [];
          });
          return;
        }

        let inflight = cacheState.inflight.get(cacheKey);
        if (!inflight) {
          inflight = client.getAgents(userId, { enabled: true })
            .then((memoryAgents) => (memoryAgents || []).map(normalizeAgent))
            .then((agents) => {
              syncLoadedAgents(userId, agents);
              return agents;
            })
            .finally(() => {
              cacheState.inflight.delete(cacheKey);
            });
          cacheState.inflight.set(cacheKey, inflight);
        }

        const agents = await inflight;
        debugLog('🔍 [Memory] loadAgents 返回的数据:', agents);
        set((state: ChatStoreDraft) => {
          state.agents = agents || [];
        });
      } catch (error) {
        console.error('Failed to load agents:', error);
        set((state: ChatStoreDraft) => {
          state.agents = [];
          state.error = error instanceof Error ? error.message : 'Failed to load agents';
        });
      }
    },

    createAgent: async (agent: AgentConfig) => {
      try {
        const userId = getUserIdParam();
        const created = await client.createAgent({
          user_id: userId,
          name: agent.name,
          description: agent.description ?? null,
          category: agent.project_policy?.category as string | undefined ?? null,
          role_definition: agent.role_definition || '',
          plugin_sources: Array.isArray(agent.plugin_sources) ? agent.plugin_sources : [],
          skills: Array.isArray(agent.skills) ? agent.skills.map((skill) => ({
            id: String(skill?.id || '').trim(),
            name: String(skill?.name || '').trim(),
            content: String(skill?.content || '').trim(),
          })).filter((skill) => skill.id && skill.name && skill.content) : [],
          skill_ids: Array.isArray(agent.skill_ids) ? agent.skill_ids : [],
          default_skill_ids: Array.isArray(agent.default_skill_ids) ? agent.default_skill_ids : [],
          mcp_policy: agent.mcp_policy ?? null,
          project_policy: agent.project_policy ?? null,
          enabled: agent.enabled !== false,
        });
        const normalized = normalizeAgent(created);
        const next = [...get().agents, normalized];
        syncLoadedAgents(userId, next);
        set((state: ChatStoreDraft) => {
          state.agents = next;
        });
        return normalized;
      } catch (error) {
        console.error('Failed to create agent:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to create agent';
        });
        return null;
      }
    },

    updateAgent: async (agent: AgentConfig) => {
      try {
        const userId = getUserIdParam();
        const saved = await client.updateAgent(agent.id, {
          name: agent.name,
          description: agent.description ?? null,
          role_definition: agent.role_definition || '',
          plugin_sources: Array.isArray(agent.plugin_sources) ? agent.plugin_sources : [],
          skills: Array.isArray(agent.skills) ? agent.skills.map((skill) => ({
            id: String(skill?.id || '').trim(),
            name: String(skill?.name || '').trim(),
            content: String(skill?.content || '').trim(),
          })).filter((skill) => skill.id && skill.name && skill.content) : [],
          skill_ids: Array.isArray(agent.skill_ids) ? agent.skill_ids : [],
          default_skill_ids: Array.isArray(agent.default_skill_ids) ? agent.default_skill_ids : [],
          mcp_policy: agent.mcp_policy ?? null,
          project_policy: agent.project_policy ?? null,
          enabled: agent.enabled !== false,
        });
        const normalized = normalizeAgent(saved);
        const next = get().agents.map((item) => (item.id === normalized.id ? normalized : item));
        syncLoadedAgents(userId, next);
        set((state: ChatStoreDraft) => {
          state.agents = next;
        });
        return normalized;
      } catch (error) {
        console.error('Failed to update agent:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to update agent';
        });
        return null;
      }
    },

    deleteAgent: async (agentId: string) => {
      try {
        const userId = getUserIdParam();
        await client.deleteAgent(agentId);
        const next = get().agents.filter((item) => item.id !== agentId);
        syncLoadedAgents(userId, next);
        set((state: ChatStoreDraft) => {
          state.agents = next;
          if (state.selectedAgentId === agentId) {
            state.selectedAgentId = null;
          }
        });
      } catch (error) {
        console.error('Failed to delete agent:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete agent';
        });
      }
    },

    aiCreateAgent: async (payload: AiCreateAgentActionPayload) => {
      try {
        const userId = getUserIdParam();
        const created = await client.aiCreateAgent({
          user_id: userId,
          model_config_id: payload.model_config_id,
          requirement: payload.requirement,
          name: payload.name,
          category: payload.category,
          description: payload.description,
          role_definition: payload.role_definition,
          skill_ids: payload.skill_ids,
          skill_prompts: payload.skill_prompts,
          enabled: payload.enabled,
          mcp_enabled: payload.mcp_enabled,
          enabled_mcp_ids: payload.enabled_mcp_ids,
          project_id: payload.project_id,
          project_root: payload.project_root,
        });
        const normalized = normalizeAgent({
          ...created.agent,
          id: created.agent?.id || generateId(),
        });
        const next = [...get().agents.filter((item) => item.id !== normalized.id), normalized];
        syncLoadedAgents(userId, next);
        set((state: ChatStoreDraft) => {
          state.agents = next;
        });
        return normalized;
      } catch (error) {
        console.error('Failed to AI-create agent:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to AI-create agent';
        });
        return null;
      }
    },

    setSelectedAgent: (agentId: string | null) => {
      let sessionIdToPersist: string | null = null;
      let metadataToPersist: Record<string, unknown> | null = null;

      set((state: ChatStoreDraft) => {
        state.selectedAgentId = agentId;
        const sessionId = state.currentSessionId;
        if (sessionId) {
          const nextSelection = {
            selectedModelId: state.selectedModelId ?? null,
            selectedAgentId: state.selectedAgentId ?? null,
          };
          if (!state.sessionAiSelectionBySession) {
            state.sessionAiSelectionBySession = {};
          }
          state.sessionAiSelectionBySession[sessionId] = nextSelection;

          const sessionIndex = state.sessions.findIndex((session) => session.id === sessionId);
          const baseMetadata = sessionIndex >= 0
            ? state.sessions[sessionIndex]?.metadata
            : state.currentSession?.metadata;
          const metadataWithSelection = mergeSessionAiSelectionIntoMetadata(baseMetadata, nextSelection);
          const nextMetadata = mergeSessionRuntimeIntoMetadata(metadataWithSelection, {
            contactAgentId: nextSelection.selectedAgentId,
            selectedModelId: nextSelection.selectedModelId,
          });
          if (sessionIndex >= 0) {
            state.sessions[sessionIndex].metadata = nextMetadata;
          }
          if (state.currentSession?.id === sessionId) {
            state.currentSession.metadata = nextMetadata;
          }

          sessionIdToPersist = sessionId;
          metadataToPersist = nextMetadata;
        }
      });

      if (sessionIdToPersist) {
        void client
          .updateSession(sessionIdToPersist, { metadata: metadataToPersist })
          .catch(() => {});
      }
    },
  };
}
