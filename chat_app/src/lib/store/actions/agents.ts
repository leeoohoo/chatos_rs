import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';

interface Deps {
  set: any;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createAgentActions({ set, client, getUserIdParam }: Deps) {
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
      set((state: any) => {
        state.selectedAgentId = agentId;
        if (agentId) {
          state.selectedModelId = null;
        }
      });
    },
  };
}
