import type ApiClient from '../../api/client';
import { normalizeAgent } from '../../domain/configs';
import { mergeSessionAiSelectionIntoMetadata } from '../helpers/sessionAiSelection';
import { mergeSessionRuntimeIntoMetadata } from '../helpers/sessionRuntime';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';
import { debugLog } from '@/lib/utils';
import type { AgentConfig } from '../../../types';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}

interface LoadAgentsOptions {
  force?: boolean;
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
  void get;

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
          inflight = client.getMemoryAgents(userId, { enabled: true })
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
