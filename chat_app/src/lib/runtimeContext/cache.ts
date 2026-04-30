import type { TurnRuntimeSnapshotLookupResponse } from '../api/client/types';

export interface RuntimeContextApiClientLike {
  getConversationLatestTurnRuntimeContext: (
    sessionId: string,
  ) => Promise<TurnRuntimeSnapshotLookupResponse>;
}

interface RuntimeContextCacheEntry {
  data: TurnRuntimeSnapshotLookupResponse;
  stale: boolean;
}

interface RuntimeContextClientCacheState {
  cache: Map<string, RuntimeContextCacheEntry>;
  inflight: Map<string, Promise<TurnRuntimeSnapshotLookupResponse>>;
}

const runtimeContextCaches = new WeakMap<RuntimeContextApiClientLike, RuntimeContextClientCacheState>();

const normalizeSessionId = (sessionId: string): string => String(sessionId || '').trim();

const getOrCreateClientCacheState = (apiClient: RuntimeContextApiClientLike): RuntimeContextClientCacheState => {
  const existing = runtimeContextCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: RuntimeContextClientCacheState = {
    cache: new Map(),
    inflight: new Map(),
  };
  runtimeContextCaches.set(apiClient, next);
  return next;
};

export const peekRuntimeContextCacheEntry = (
  apiClient: RuntimeContextApiClientLike,
  sessionId: string,
): RuntimeContextCacheEntry | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreateClientCacheState(apiClient).cache.get(normalizedSessionId) || null;
};

export const getCachedRuntimeContextData = (
  apiClient: RuntimeContextApiClientLike,
  sessionId: string,
): TurnRuntimeSnapshotLookupResponse | null => {
  return peekRuntimeContextCacheEntry(apiClient, sessionId)?.data || null;
};

export const markRuntimeContextStale = (
  apiClient: RuntimeContextApiClientLike,
  sessionId: string,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateClientCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cacheState.cache.set(normalizedSessionId, {
    ...cached,
    stale: true,
  });
};

export const loadRuntimeContextSnapshot = async (
  apiClient: RuntimeContextApiClientLike,
  sessionId: string,
  options?: { force?: boolean },
): Promise<TurnRuntimeSnapshotLookupResponse> => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    throw new Error('session id is required');
  }

  const cacheState = getOrCreateClientCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!options?.force && cached && !cached.stale) {
    return cached.data;
  }

  let inflight = cacheState.inflight.get(normalizedSessionId);
  if (!inflight) {
    inflight = apiClient.getConversationLatestTurnRuntimeContext(normalizedSessionId)
      .then((payload) => {
        cacheState.cache.set(normalizedSessionId, {
          data: payload,
          stale: false,
        });
        return payload;
      })
      .finally(() => {
        cacheState.inflight.delete(normalizedSessionId);
      });
    cacheState.inflight.set(normalizedSessionId, inflight);
  }

  return inflight;
};
