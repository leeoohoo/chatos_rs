import type { McpConfig } from '../../../types';
import type ApiClient from '../../api/client';
import { debugLog } from '@/lib/utils';

interface Deps {
  set: any;
  get: any;
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

        set((state: any) => {
          state.mcpConfigs = configs as McpConfig[];
        });
      } catch (error) {
        console.error('Failed to load MCP configs:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load MCP configs';
        });
      }
    },

    updateMcpConfig: async (config: McpConfig) => {
      try {
        const userId = getUserIdParam();
        debugLog('🔍 updateMcpConfig 调用:', {
          userId,
          configId: (config as any).id,
          configName: (config as any).name,
        });

        let saved: McpConfig | null = null;
        if ((config as any).id) {
          const updateData: any = {
            id: (config as any).id,
            name: (config as any).name,
            command: (config as any).command,
            type: (config as any).type,
            args: (config as any).args ?? undefined,
            env: (config as any).env ?? undefined,
            cwd: (config as any).cwd ?? undefined,
            enabled: (config as any).enabled,
            userId
          };
          debugLog('🔍 updateMcpConfig 更新数据:', updateData);
          saved = await (client as any).updateMcpConfig((config as any).id, updateData);
        } else {
          // 如果没有 id，视为创建
          const createData: any = {
            name: (config as any).name,
            command: (config as any).command,
            type: (config as any).type,
            args: (config as any).args ?? undefined,
            env: (config as any).env ?? undefined,
            cwd: (config as any).cwd ?? undefined,
            enabled: (config as any).enabled,
            user_id: userId
          };
          saved = await (client as any).createMcpConfig(createData);
        }

        // 重新加载配置
        await get().loadMcpConfigs();

        return saved;
      } catch (error) {
        console.error('Failed to update MCP config:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update MCP config';
        });
        return null;
      }
    },

    deleteMcpConfig: async (id: string) => {
      try {
        await client.deleteMcpConfig(id);
        set((state: any) => {
          state.mcpConfigs = state.mcpConfigs.filter((config: any) => config.id !== id);
        });
      } catch (error) {
        console.error('Failed to delete MCP config:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete MCP config';
        });
      }
    },
  };
}
