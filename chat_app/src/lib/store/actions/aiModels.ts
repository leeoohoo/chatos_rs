import type { AiModelConfig } from '../../../types';
import type {
  AiModelConfigCreatePayload,
  AiModelConfigResponse,
} from '../../api/client/types';
import type ApiClient from '../../api/client';
import { mergeSessionAiSelectionIntoMetadata } from '../helpers/sessionAiSelection';
import { mergeSessionRuntimeIntoMetadata } from '../helpers/sessionRuntime';
import type { ChatActions, ChatState, ChatStoreDraft } from '../types';
import { generateId } from '@/lib/utils';

interface Deps {
  set: (updater: (state: ChatStoreDraft) => void) => void;
  get: () => ChatState & ChatActions;
  client: ApiClient;
  getUserIdParam: () => string;
}

const toDate = (value?: string): Date => {
  if (!value) {
    return new Date();
  }

  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? new Date() : parsed;
};

const normalizeAiModelConfig = (config: AiModelConfigResponse): AiModelConfig => {
  const createdAt = config.created_at || config.createdAt;
  const updatedAt = config.updated_at || config.updatedAt || createdAt;

  return {
    id: config.id,
    name: config.name,
    provider: config.provider || 'gpt',
    base_url: config.base_url || '',
    api_key: config.api_key || '',
    model_name: config.model_name || config.model || '',
    thinking_level: config.thinking_level || undefined,
    enabled: config.enabled === true,
    supports_images: config.supports_images === true,
    supports_reasoning: config.supports_reasoning === true,
    supports_responses: config.supports_responses === true,
    createdAt: toDate(createdAt),
    updatedAt: toDate(updatedAt),
  };
};

export function createAiModelActions({ set, get, client, getUserIdParam }: Deps) {
  return {
    loadAiModelConfigs: async () => {
      try {
        const userId = getUserIdParam();
        const configs = (await client.getAiModelConfigs(userId)).map(normalizeAiModelConfig);
        set((state) => {
          state.aiModelConfigs = configs;
        });
      } catch (error) {
        console.error('Failed to load AI model configs:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to load AI model configs';
        });
      }
    },
    updateAiModelConfig: async (config: AiModelConfig) => {
      try {
        const userId = getUserIdParam();
        const existingConfig = get().aiModelConfigs.find((item) => item.id === config.id);
        const method = existingConfig ? 'update' : 'create';
        const provider = config.provider || 'gpt';
        const thinking_level = provider === 'gpt' ? (config.thinking_level || undefined) : undefined;
        const apiData: AiModelConfigCreatePayload = {
          id: config.id || generateId(),
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
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to update AI model config';
        });
      }
    },
    deleteAiModelConfig: async (id: string) => {
      try {
        await client.deleteAiModelConfig(id);
        set((state) => {
          state.aiModelConfigs = state.aiModelConfigs.filter((item) => item.id !== id);
        });
      } catch (error) {
        console.error('Failed to delete AI model config:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to delete AI model config';
        });
      }
    },
    setSelectedModel: (modelId: string | null) => {
      let sessionIdToPersist: string | null = null;
      let metadataToPersist: Record<string, unknown> | null = null;

      set((state) => {
        state.selectedModelId = modelId;
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
