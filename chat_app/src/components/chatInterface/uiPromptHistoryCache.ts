import type { UiPromptHistoryItem } from './types';
import {
  getSessionScopedInflight,
  markSessionScopedCacheStale,
  peekSessionScopedCacheEntry,
  setSessionScopedCacheEntry,
  setSessionScopedInflight,
  type SessionScopedCacheState,
} from './sessionScopedCache';

interface UiPromptHistoryCacheEntry {
  items: UiPromptHistoryItem[];
  stale: boolean;
}

interface UiPromptHistoryApiClientLike {
  getUiPromptHistory: (
    sessionId: string,
    params?: { limit?: number },
  ) => Promise<unknown[]>;
}

const uiPromptHistoryCaches = new WeakMap<
  UiPromptHistoryApiClientLike,
  SessionScopedCacheState<UiPromptHistoryItem[]>
>();

const getOrCreateUiPromptHistoryCacheState = (
  apiClient: UiPromptHistoryApiClientLike,
): SessionScopedCacheState<UiPromptHistoryItem[]> => {
  const existing = uiPromptHistoryCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: SessionScopedCacheState<UiPromptHistoryItem[]> = {
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
  const cached = peekSessionScopedCacheEntry(
    getOrCreateUiPromptHistoryCacheState(apiClient).cache,
    sessionId,
  );
  return cached
    ? {
      items: cached.value,
      stale: cached.stale,
    }
    : null;
};

export const setUiPromptHistoryCacheEntry = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
  items: UiPromptHistoryItem[],
): void => {
  setSessionScopedCacheEntry(
    getOrCreateUiPromptHistoryCacheState(apiClient).cache,
    sessionId,
    items,
  );
};

export const markUiPromptHistoryCacheStale = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
): void => {
  markSessionScopedCacheStale(
    getOrCreateUiPromptHistoryCacheState(apiClient).cache,
    sessionId,
  );
};

export const getUiPromptHistoryInflight = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
): Promise<UiPromptHistoryItem[]> | null => {
  return getSessionScopedInflight(
    getOrCreateUiPromptHistoryCacheState(apiClient).inflight,
    sessionId,
  );
};

export const setUiPromptHistoryInflight = (
  apiClient: UiPromptHistoryApiClientLike,
  sessionId: string,
  inflight: Promise<UiPromptHistoryItem[]> | null,
): void => {
  setSessionScopedInflight(
    getOrCreateUiPromptHistoryCacheState(apiClient).inflight,
    sessionId,
    inflight,
  );
};
