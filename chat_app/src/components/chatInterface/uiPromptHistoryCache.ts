import type { UiPromptHistoryItem } from './types';

interface UiPromptHistoryCacheEntry {
  items: UiPromptHistoryItem[];
  stale: boolean;
}

interface UiPromptHistoryCacheState {
  cache: Map<string, UiPromptHistoryCacheEntry>;
  inflight: Map<string, Promise<UiPromptHistoryItem[]>>;
}

interface UiPromptHistoryApiClientLike {
  getUiPromptHistory: (
    sessionId: string,
    params?: { limit?: number },
  ) => Promise<unknown[]>;
}

const uiPromptHistoryCaches = new WeakMap<UiPromptHistoryApiClientLike, UiPromptHistoryCacheState>();

const normalizeSessionId = (sessionId: string): string => String(sessionId || '').trim();

const getOrCreateUiPromptHistoryCacheState = (
  apiClient: UiPromptHistoryApiClientLike,
): UiPromptHistoryCacheState => {
  const existing = uiPromptHistoryCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: UiPromptHistoryCacheState = {
    cache: new Map(),
    inflight: new Map(),
  };
  uiPromptHistoryCaches.set(apiClient, next);
  return next;
};

export const peekUiPromptHistoryCacheEntry = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
): UiPromptHistoryCacheEntry | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreateUiPromptHistoryCacheState(apiClient).cache.get(normalizedSessionId) || null;
};

export const setUiPromptHistoryCacheEntry = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
  items: UiPromptHistoryItem[],
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  getOrCreateUiPromptHistoryCacheState(apiClient).cache.set(normalizedSessionId, {
    items,
    stale: false,
  });
};

export const markUiPromptHistoryCacheStale = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateUiPromptHistoryCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cacheState.cache.set(normalizedSessionId, {
    ...cached,
    stale: true,
  });
};

export const getUiPromptHistoryInflight = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
): Promise<UiPromptHistoryItem[]> | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreateUiPromptHistoryCacheState(apiClient).inflight.get(normalizedSessionId) || null;
};

export const setUiPromptHistoryInflight = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
  inflight: Promise<UiPromptHistoryItem[]> | null,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateUiPromptHistoryCacheState(apiClient);
  if (inflight) {
    cacheState.inflight.set(normalizedSessionId, inflight);
    return;
  }
  cacheState.inflight.delete(normalizedSessionId);
};
