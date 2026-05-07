import type { SessionSummariesListResponse } from '../api/client/types';
import {
  normalizeSessionSummary,
  type SessionSummaryItem,
} from '../domain/configs';

export interface ConversationSummaryApiClientLike {
  getConversationSummaries: (
    sessionId: string,
    options?: { limit?: number; offset?: number },
  ) => Promise<SessionSummariesListResponse>;
}

interface ConversationSummaryCacheEntry {
  items: SessionSummaryItem[];
  stale: boolean;
  loadedLimit: number;
}

interface ConversationSummaryInflightEntry {
  limit: number;
  promise: Promise<SessionSummaryItem[]>;
}

interface ConversationSummaryClientCacheState {
  cache: Map<string, ConversationSummaryCacheEntry>;
  inflight: Map<string, ConversationSummaryInflightEntry>;
}

const DEFAULT_SUMMARY_LIMIT = 300;

const conversationSummaryCaches = new WeakMap<
  ConversationSummaryApiClientLike,
  ConversationSummaryClientCacheState
>();

const normalizeSessionId = (sessionId: string): string => String(sessionId || '').trim();

const normalizeLimit = (limit?: number): number => {
  const numeric = Number(limit);
  if (!Number.isFinite(numeric) || numeric <= 0) {
    return DEFAULT_SUMMARY_LIMIT;
  }
  return Math.max(1, Math.floor(numeric));
};

const getOrCreateConversationSummaryCacheState = (
  apiClient: ConversationSummaryApiClientLike,
): ConversationSummaryClientCacheState => {
  const existing = conversationSummaryCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: ConversationSummaryClientCacheState = {
    cache: new Map(),
    inflight: new Map(),
  };
  conversationSummaryCaches.set(apiClient, next);
  return next;
};

export const normalizeConversationSummaryItems = (
  payload: SessionSummariesListResponse | unknown,
): SessionSummaryItem[] => (Array.isArray((payload as SessionSummariesListResponse | null | undefined)?.items)
  ? (payload as SessionSummariesListResponse).items
  : []
)
  .map((item) => normalizeSessionSummary(item))
  .filter((item): item is SessionSummaryItem => Boolean(item))
  .sort((left, right) => {
    const leftTs = new Date(left.createdAt || left.updatedAt).getTime();
    const rightTs = new Date(right.createdAt || right.updatedAt).getTime();
    return (Number.isFinite(rightTs) ? rightTs : 0) - (Number.isFinite(leftTs) ? leftTs : 0);
  });

export const peekConversationSummaryCacheEntry = (
  apiClient: ConversationSummaryApiClientLike,
  sessionId: string,
): ConversationSummaryCacheEntry | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreateConversationSummaryCacheState(apiClient).cache.get(normalizedSessionId) || null;
};

export const getCachedConversationSummaryItems = (
  apiClient: ConversationSummaryApiClientLike,
  sessionId: string,
): SessionSummaryItem[] | null => {
  const cached = peekConversationSummaryCacheEntry(apiClient, sessionId);
  return cached ? [...cached.items] : null;
};

export const markConversationSummaryCacheStale = (
  apiClient: ConversationSummaryApiClientLike,
  sessionId: string,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateConversationSummaryCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cacheState.cache.set(normalizedSessionId, {
    ...cached,
    stale: true,
  });
};

export const setConversationSummaryCacheEntry = (
  apiClient: ConversationSummaryApiClientLike,
  sessionId: string,
  items: SessionSummaryItem[],
  options?: { loadedLimit?: number; stale?: boolean },
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  getOrCreateConversationSummaryCacheState(apiClient).cache.set(normalizedSessionId, {
    items: [...items],
    stale: options?.stale === true,
    loadedLimit: normalizeLimit(options?.loadedLimit),
  });
};

export const applyConversationSummaryItemsSnapshot = (
  apiClient: ConversationSummaryApiClientLike,
  sessionId: string,
  payload: SessionSummariesListResponse | unknown,
  options?: { loadedLimit?: number },
): SessionSummaryItem[] => {
  const normalized = normalizeConversationSummaryItems(payload);
  setConversationSummaryCacheEntry(apiClient, sessionId, normalized, {
    loadedLimit: options?.loadedLimit,
    stale: false,
  });
  return normalized;
};

export const loadConversationSummaryItems = (
  apiClient: ConversationSummaryApiClientLike,
  sessionId: string,
  options?: { force?: boolean; limit?: number },
): Promise<SessionSummaryItem[]> => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return Promise.resolve([]);
  }

  const requestedLimit = normalizeLimit(options?.limit);
  const cacheState = getOrCreateConversationSummaryCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (
    !options?.force
    && cached
    && !cached.stale
    && cached.loadedLimit >= requestedLimit
  ) {
    return Promise.resolve([...cached.items]);
  }

  const existingInflight = options?.force
    ? null
    : cacheState.inflight.get(normalizedSessionId);
  if (
    existingInflight
    && existingInflight.limit >= requestedLimit
  ) {
    return existingInflight.promise.then((items) => [...items]);
  }

  const inflightPromise = apiClient
    .getConversationSummaries(normalizedSessionId, { limit: requestedLimit, offset: 0 })
    .then((result) => {
      const normalized = normalizeConversationSummaryItems(result);
      cacheState.cache.set(normalizedSessionId, {
        items: normalized,
        stale: false,
        loadedLimit: requestedLimit,
      });
      return normalized;
    })
    .finally(() => {
      const currentInflight = cacheState.inflight.get(normalizedSessionId);
      if (currentInflight?.promise === inflightPromise) {
        cacheState.inflight.delete(normalizedSessionId);
      }
    });

  cacheState.inflight.set(normalizedSessionId, {
    limit: requestedLimit,
    promise: inflightPromise,
  });

  return inflightPromise.then((items) => [...items]);
};
