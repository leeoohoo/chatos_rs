import type { AiModelConfig } from '../../../types';
import type {
  AiModelConfigCreatePayload,
  AiModelConfigUpdatePayload,
} from '../../api/client/types';
import type ApiClient from '../../api/client';
import { normalizeAiModelConfig } from '../../domain/configs';
import { mergeSessionAiSelectionIntoMetadata } from '../helpers/sessionAiSelection';
import { mergeSessionRuntimeIntoMetadata } from '../helpers/sessionRuntime';
import type { ChatActions, ChatState, ChatStoreDraft } from '../types';
import { generateId } from '@/lib/utils';

interface Deps {
  set: (updater: (state: ChatStoreDraft) => void) => void;
  get: () => ChatState & ChatActions;
  client: ApiClient;
}

interface LoadAiModelConfigsOptions {
  force?: boolean;
}

interface AiModelConfigsCacheEntry {
  configs: AiModelConfig[];
  stale: boolean;
}

interface AiModelConfigsClientCacheState {
  cache: AiModelConfigsCacheEntry | null;
  inflight: Promise<AiModelConfig[]> | null;
}

const aiModelConfigsClientCaches = new WeakMap<ApiClient, AiModelConfigsClientCacheState>();

const getOrCreateClientCacheState = (apiClient: ApiClient): AiModelConfigsClientCacheState => {
  const existing = aiModelConfigsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: AiModelConfigsClientCacheState = {
    cache: null,
    inflight: null,
  };
  aiModelConfigsClientCaches.set(apiClient, next);
  return next;
};

export function createAiModelActions({ set, get, client }: Deps) {
  const syncLoadedAiModelConfigs = (configs: AiModelConfig[]) => {
    getOrCreateClientCacheState(client).cache = {
      configs,
      stale: false,
    };
  };

  return {
    loadAiModelConfigs: async (options?: LoadAiModelConfigsOptions) => {
      try {
        const cacheState = getOrCreateClientCacheState(client);
        const cached = cacheState.cache;
        if (!options?.force && cached && !cached.stale) {
          set((state) => {
            state.aiModelConfigs = cached.configs;
          });
          return;
        }

        let inflight = cacheState.inflight;
        if (!inflight) {
          inflight = client.getAiModelConfigs()
            .then((list) => list.map(normalizeAiModelConfig))
            .then((configs) => {
              syncLoadedAiModelConfigs(configs);
              return configs;
            })
            .finally(() => {
              cacheState.inflight = null;
            });
          cacheState.inflight = inflight;
        }

        const configs = await inflight;
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
    updateAiModelConfig: async (
      config: AiModelConfig,
      options?: { clearApiKey?: boolean },
    ) => {
      try {
        const existingConfig = get().aiModelConfigs.find((item) => item.id === config.id);
        const method = existingConfig ? 'update' : 'create';
        const provider = config.provider || 'gpt';
        const trimmedApiKey = config.api_key.trim();
        const apiData: AiModelConfigCreatePayload = {
          id: config.id || generateId(),
          name: config.name,
          provider,
          model: config.model_name.trim() || undefined,
          api_key: trimmedApiKey,
          base_url: config.base_url,
          enabled: config.enabled,
          supports_images: config.supports_images === true,
          supports_reasoning: config.supports_reasoning === true,
          supports_responses: config.supports_responses === true,
          task_usage_scenario: config.task_usage_scenario?.trim() || undefined,
          task_thinking_level: config.task_thinking_level?.trim() || undefined,
        };
        if (method === 'update') {
          const updateData: AiModelConfigUpdatePayload = {
            id: apiData.id,
            name: apiData.name,
            provider: apiData.provider,
            model: apiData.model,
            base_url: apiData.base_url,
            enabled: apiData.enabled,
            supports_images: apiData.supports_images,
            supports_reasoning: apiData.supports_reasoning,
            supports_responses: apiData.supports_responses,
            task_usage_scenario: apiData.task_usage_scenario,
            task_thinking_level: apiData.task_thinking_level,
          };
          if (trimmedApiKey) {
            updateData.api_key = trimmedApiKey;
          }
          if (options?.clearApiKey) {
            updateData.clear_api_key = true;
          }
          await client.updateAiModelConfig(config.id!, updateData);
        } else {
          await client.createAiModelConfig(apiData);
        }
        const cacheState = getOrCreateClientCacheState(client);
        const cached = cacheState.cache;
        if (cached) {
          cacheState.cache = {
            ...cached,
            stale: true,
          };
        }
        await get().loadAiModelConfigs({ force: true });
      } catch (error) {
        console.error('Failed to update AI model config:', error);
        set((state) => {
          state.error = error instanceof Error ? error.message : 'Failed to update AI model config';
        });
        throw error;
      }
    },
    deleteAiModelConfig: async (id: string) => {
      try {
        await client.deleteAiModelConfig(id);
        const cacheState = getOrCreateClientCacheState(client);
        if (cacheState.cache) {
          cacheState.cache = {
            ...cacheState.cache,
            configs: cacheState.cache.configs.filter((item) => item.id !== id),
            stale: false,
          };
        }
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
