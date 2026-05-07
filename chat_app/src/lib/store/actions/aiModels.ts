import type { AiModelConfig } from '../../../types';
import type {
  AiModelConfigCreatePayload,
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
  getUserIdParam: () => string;
}

interface LoadAiModelConfigsOptions {
  force?: boolean;
}

interface AiModelConfigsCacheEntry {
  configs: AiModelConfig[];
  stale: boolean;
}

interface AiModelConfigsClientCacheState {
  cache: Map<string, AiModelConfigsCacheEntry>;
  inflight: Map<string, Promise<AiModelConfig[]>>;
}

const aiModelConfigsClientCaches = new WeakMap<ApiClient, AiModelConfigsClientCacheState>();

const normalizeUserId = (userId: string): string => String(userId || '').trim();

const getOrCreateClientCacheState = (apiClient: ApiClient): AiModelConfigsClientCacheState => {
  const existing = aiModelConfigsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: AiModelConfigsClientCacheState = {
    cache: new Map(),
    inflight: new Map(),
  };
  aiModelConfigsClientCaches.set(apiClient, next);
  return next;
};

export function createAiModelActions({ set, get, client, getUserIdParam }: Deps) {
  const syncLoadedAiModelConfigs = (userId: string, configs: AiModelConfig[]) => {
    getOrCreateClientCacheState(client).cache.set(normalizeUserId(userId), {
      configs,
      stale: false,
    });
  };

  return {
    loadAiModelConfigs: async (options?: LoadAiModelConfigsOptions) => {
      try {
        const userId = getUserIdParam();
        const cacheKey = normalizeUserId(userId);
        const cacheState = getOrCreateClientCacheState(client);
        const cached = cacheState.cache.get(cacheKey);
        if (!options?.force && cached && !cached.stale) {
          set((state) => {
            state.aiModelConfigs = cached.configs;
          });
          return;
        }

        let inflight = cacheState.inflight.get(cacheKey);
        if (!inflight) {
          inflight = client.getAiModelConfigs(userId)
            .then((list) => list.map(normalizeAiModelConfig))
            .then((configs) => {
              syncLoadedAiModelConfigs(userId, configs);
              return configs;
            })
            .finally(() => {
              cacheState.inflight.delete(cacheKey);
            });
          cacheState.inflight.set(cacheKey, inflight);
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
        const cacheState = getOrCreateClientCacheState(client);
        const cacheKey = normalizeUserId(userId);
        const cached = cacheState.cache.get(cacheKey);
        if (cached) {
          cacheState.cache.set(cacheKey, {
            ...cached,
            stale: true,
          });
        }
        await get().loadAiModelConfigs({ force: true });
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
        const cacheState = getOrCreateClientCacheState(client);
        cacheState.cache.forEach((entry, key) => {
          cacheState.cache.set(key, {
            configs: entry.configs.filter((item) => item.id !== id),
            stale: false,
          });
        });
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
