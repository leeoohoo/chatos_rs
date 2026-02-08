import type { AiModelConfig } from '../../../types';
import type ApiClient from '../../api/client';

export function createAiModelActions({ set, get, client, getUserIdParam }: { set: any; get: any; client: ApiClient; getUserIdParam: () => string; }) {
  return {
    loadAiModelConfigs: async () => {
      try {
        const userId = getUserIdParam();
        const apiConfigs = (await client.getAiModelConfigs(userId)) as any[];
        const configs = apiConfigs.map((config: any) => ({
          id: config.id,
          name: config.name,
          provider: config.provider || 'gpt',
          base_url: config.base_url,
          api_key: config.api_key,
          model_name: config.model,
          thinking_level: config.thinking_level || undefined,
          enabled: config.enabled,
          supports_images: config.supports_images === true,
          supports_reasoning: config.supports_reasoning === true,
          supports_responses: config.supports_responses === true,
          createdAt: new Date(config.created_at),
          updatedAt: new Date(config.created_at),
        }));
        set((state: any) => {
          state.aiModelConfigs = configs;
        });
      } catch (error) {
        console.error('Failed to load AI model configs:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to load AI model configs';
        });
      }
    },
    updateAiModelConfig: async (config: AiModelConfig) => {
      try {
        const userId = getUserIdParam();
        const existingConfig = get().aiModelConfigs.find((c: any) => c.id === config.id);
        const method = existingConfig ? 'update' : 'create';
        const provider = config.provider || 'gpt';
        const thinking_level = provider === 'gpt' ? (config.thinking_level || undefined) : undefined;
        const apiData = {
          id: config.id || crypto.randomUUID(),
          name: config.name,
          provider,
          model: config.model_name,
          thinking_level,
          api_key: config.api_key,
          base_url: config.base_url,
          enabled: config.enabled,
          supports_images: config.supports_images === true,
          supports_reasoning: config.supports_reasoning === true,
          supports_responses: config.supports_responses === true,
          user_id: userId,
        };
        if (method === 'update') {
          await client.updateAiModelConfig(config.id!, apiData);
        } else {
          await client.createAiModelConfig(apiData);
        }
        await get().loadAiModelConfigs();
      } catch (error) {
        console.error('Failed to update AI model config:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to update AI model config';
        });
      }
    },
    deleteAiModelConfig: async (id: string) => {
      try {
        await client.deleteAiModelConfig(id);
        set((state: any) => {
          state.aiModelConfigs = state.aiModelConfigs.filter((c: any) => c.id !== id);
        });
      } catch (error) {
        console.error('Failed to delete AI model config:', error);
        set((state: any) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete AI model config';
        });
      }
    },
    setSelectedModel: (modelId: string | null) => {
      set((state: any) => {
        state.selectedModelId = modelId;
        if (modelId) {
          state.selectedAgentId = null;
        }
      });
    },
  };
}
