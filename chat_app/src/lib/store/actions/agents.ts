import type ApiClient from '../../api/client';
import { mergeSessionAiSelectionIntoMetadata } from '../helpers/sessionAiSelection';
import { debugLog } from '@/lib/utils';

interface Deps {
  set: any;
  get: any;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createAgentActions({ set, get, client, getUserIdParam }: Deps) {
  void get;
  return {
    loadAgents: async () => {
      try {
        const agents = await client.getAgents(getUserIdParam());
        debugLog('🔍 [后端返回] loadAgents 返回的数据:', agents);
        debugLog('🔍 [后端返回] 第一个智能体的 app_ids:', agents?.[0]?.app_ids);
        set((state: any) => {
          state.agents = (agents || []) as any[];
        });
      } catch (error) {
        console.error('Failed to load agents:', error);
        set((state: any) => {
          state.agents = [];
          state.error = error instanceof Error ? error.message : 'Failed to load agents';
        });
      }
    },

    setSelectedAgent: (agentId: string | null) => {
      let sessionIdToPersist: string | null = null;
      let metadataToPersist: Record<string, any> | null = null;

      set((state: any) => {
        state.selectedAgentId = agentId;
        if (agentId) {
          state.selectedModelId = null;
        }
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

          const sessionIndex = state.sessions.findIndex((s: any) => s.id === sessionId);
          const baseMetadata = sessionIndex >= 0
            ? state.sessions[sessionIndex]?.metadata
            : state.currentSession?.metadata;
          const nextMetadata = mergeSessionAiSelectionIntoMetadata(baseMetadata, nextSelection);
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
