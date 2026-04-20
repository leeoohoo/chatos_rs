import type ApiClient from '../../api/client';
import { mergeSessionAiSelectionIntoMetadata } from '../helpers/sessionAiSelection';
import { mergeSessionRuntimeIntoMetadata } from '../helpers/sessionRuntime';
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
        const memoryAgents = await client.getMemoryAgents(getUserIdParam(), { enabled: true });
        const agents = (memoryAgents || []).map((agent: any) => ({
          id: agent.id,
          name: agent.name,
          description: agent.description || '',
          ai_model_config_id: '',
          enabled: agent.enabled !== false,
          project_id: agent?.project_policy?.project_id || null,
          workspace_dir: agent?.project_policy?.project_root || null,
          app_ids: [],
          role_definition: agent.role_definition || '',
          skills: Array.isArray(agent.skills) ? agent.skills : [],
          skill_ids: Array.isArray(agent.skill_ids) ? agent.skill_ids : [],
          default_skill_ids: Array.isArray(agent.default_skill_ids) ? agent.default_skill_ids : [],
          mcp_policy: agent.mcp_policy || null,
          project_policy: agent.project_policy || null,
          createdAt: agent.created_at || new Date().toISOString(),
          updatedAt: agent.updated_at || new Date().toISOString(),
        }));
        debugLog('🔍 [Memory] loadAgents 返回的数据:', agents);
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
