import type { McpConfig } from '../../../types';
import type { McpConfigResponse, McpConfigUpdatePayload } from '../../api/client/types';
import type ApiClient from '../../api/client';
import { normalizeMcpConfig } from '../../domain/configs';
import type { ChatStoreDraft, ChatStoreGet, ChatStoreSet } from '../types';
import { debugLog } from '@/lib/utils';
import { generateId } from '@/lib/utils';

interface Deps {
  set: ChatStoreSet;
  get: ChatStoreGet;
  client: ApiClient;
  getUserIdParam: () => string;
}

export function createMcpActions({ set, get, client, getUserIdParam }: Deps) {
  return {
    loadMcpConfigs: async () => {
      try {
        const userId = getUserIdParam();
        const configs = await client.getMcpConfigs(userId);

        debugLog('🔍 [后端返回] loadMcpConfigs 返回的数据:', configs);

        set((state: ChatStoreDraft) => {
          state.mcpConfigs = configs.map(normalizeMcpConfig);
        });
      } catch (error) {
        console.error('Failed to load MCP configs:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to load MCP configs';
        });
      }
    },

    updateMcpConfig: async (config: McpConfig) => {
      try {
        const userId = getUserIdParam();
        debugLog('🔍 updateMcpConfig 调用:', {
          userId,
          configId: config.id,
          configName: config.name,
        });

        let saved: McpConfigResponse | null = null;
        if (config.id) {
          const updateData: McpConfigUpdatePayload = {
            id: config.id,
            name: config.name,
            command: config.command,
            type: config.type,
            args: config.args ?? undefined,
            env: config.env ?? undefined,
            cwd: config.cwd ?? undefined,
            enabled: config.enabled,
            userId,
          };
          debugLog('🔍 updateMcpConfig 更新数据:', updateData);
          saved = await client.updateMcpConfig(config.id, updateData);
        } else {
          const createData = {
            id: generateId(),
            name: config.name,
            command: config.command,
            type: config.type,
            args: config.args ?? undefined,
            env: config.env ?? undefined,
            cwd: config.cwd ?? undefined,
            enabled: config.enabled,
            user_id: userId,
          };
          saved = await client.createMcpConfig(createData);
        }

        await get().loadMcpConfigs();

        return saved ? normalizeMcpConfig(saved) : null;
      } catch (error) {
        console.error('Failed to update MCP config:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to update MCP config';
        });
        return null;
      }
    },

    deleteMcpConfig: async (id: string) => {
      try {
        await client.deleteMcpConfig(id);
        set((state: ChatStoreDraft) => {
          state.mcpConfigs = state.mcpConfigs.filter((config) => config.id !== id);
        });
      } catch (error) {
        console.error('Failed to delete MCP config:', error);
        set((state: ChatStoreDraft) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete MCP config';
        });
      }
    },
  };
}
