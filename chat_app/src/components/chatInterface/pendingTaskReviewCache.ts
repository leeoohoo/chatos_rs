import type { TaskReviewPanelState } from '../../lib/store/types';
import { toTaskReviewPanelFromRecord } from './panelTransforms';
import {
  getSessionScopedInflight,
  markSessionScopedCacheStale,
  normalizeSessionScopedId,
  peekSessionScopedCacheEntry,
  setSessionScopedCacheEntry,
  setSessionScopedInflight,
  type SessionScopedCacheState,
} from './sessionScopedCache';

interface PendingTaskReviewCacheEntry {
  panels: TaskReviewPanelState[];
  stale: boolean;
}

interface PendingTaskReviewApiClientLike {
  getPendingTaskReviews: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
}

const pendingTaskReviewCaches = new WeakMap<
  PendingTaskReviewApiClientLike,
  SessionScopedCacheState<TaskReviewPanelState[]>
>();

const getOrCreatePendingTaskReviewCacheState = (
  apiClient: PendingTaskReviewApiClientLike,
): SessionScopedCacheState<TaskReviewPanelState[]> => {
  const existing = pendingTaskReviewCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: SessionScopedCacheState<TaskReviewPanelState[]> = {
    cache: new Map(),
    inflight: new Map(),
  };
  pendingTaskReviewCaches.set(apiClient, next);
  return next;
};

export const peekPendingTaskReviewCacheEntry = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
): PendingTaskReviewCacheEntry | null => {
  const cached = peekSessionScopedCacheEntry(
    getOrCreatePendingTaskReviewCacheState(apiClient).cache,
    sessionId,
  );
  return cached
    ? {
      panels: cached.value,
      stale: cached.stale,
    }
    : null;
};

export const setPendingTaskReviewCacheEntry = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
  panels: TaskReviewPanelState[],
): void => {
  setSessionScopedCacheEntry(
    getOrCreatePendingTaskReviewCacheState(apiClient).cache,
    sessionId,
    [...panels],
  );
};

export const upsertPendingTaskReviewCachePanel = (
  apiClient: PendingTaskReviewApiClientLike,
  panel: TaskReviewPanelState,
): void => {
  const normalizedSessionId = normalizeSessionScopedId(panel.sessionId);
  if (!normalizedSessionId || !panel.reviewId) {
    return;
  }
  const cacheState = getOrCreatePendingTaskReviewCacheState(apiClient);
  const cached = peekSessionScopedCacheEntry(cacheState.cache, normalizedSessionId);
  const nextPanels = cached ? [...cached.value] : [];
  const index = nextPanels.findIndex((item) => item.reviewId === panel.reviewId);
  if (index >= 0) {
    nextPanels[index] = panel;
  } else {
    nextPanels.push(panel);
  }
  setSessionScopedCacheEntry(cacheState.cache, normalizedSessionId, nextPanels);
};

export const removePendingTaskReviewCachePanel = (
  apiClient: PendingTaskReviewApiClientLike,
  reviewId: string,
  sessionId?: string,
): void => {
  const normalizedReviewId = String(reviewId || '').trim();
  if (!normalizedReviewId) {
    return;
  }
  const cacheState = getOrCreatePendingTaskReviewCacheState(apiClient);
  const candidateSessionIds = sessionId
    ? [normalizeSessionScopedId(sessionId)]
    : Array.from(cacheState.cache.keys());
  for (const normalizedSessionId of candidateSessionIds) {
    if (!normalizedSessionId) {
      continue;
    }
    const cached = peekSessionScopedCacheEntry(cacheState.cache, normalizedSessionId);
    if (!cached) {
      continue;
    }
    const nextPanels = cached.value.filter((panel) => panel.reviewId !== normalizedReviewId);
    if (nextPanels.length === cached.value.length) {
      continue;
    }
    setSessionScopedCacheEntry(cacheState.cache, normalizedSessionId, nextPanels);
    break;
  }
};

export const markPendingTaskReviewCacheStale = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
): void => {
  markSessionScopedCacheStale(
    getOrCreatePendingTaskReviewCacheState(apiClient).cache,
    sessionId,
  );
};

export const getPendingTaskReviewInflight = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
): Promise<TaskReviewPanelState[]> | null => {
  return getSessionScopedInflight(
    getOrCreatePendingTaskReviewCacheState(apiClient).inflight,
    sessionId,
  );
};

export const setPendingTaskReviewInflight = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
  inflight: Promise<TaskReviewPanelState[]> | null,
): void => {
  setSessionScopedInflight(
    getOrCreatePendingTaskReviewCacheState(apiClient).inflight,
    sessionId,
    inflight,
  );
};

export const loadPendingTaskReviewPanels = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
  options?: { limit?: number; force?: boolean },
): Promise<TaskReviewPanelState[]> => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return Promise.resolve([]);
  }

  const cacheState = getOrCreatePendingTaskReviewCacheState(apiClient);
  const cached = peekSessionScopedCacheEntry(cacheState.cache, normalizedSessionId);
  if (!options?.force && cached && !cached.stale) {
    return Promise.resolve([...cached.value]);
  }

  const existingInflight = getSessionScopedInflight(cacheState.inflight, normalizedSessionId);
  if (existingInflight) {
    return existingInflight;
  }

  const inflight = apiClient
    .getPendingTaskReviews(normalizedSessionId, { limit: options?.limit })
    .then((records) => {
      if (!Array.isArray(records)) {
        return [] as TaskReviewPanelState[];
      }
      const panels = records
        .map((record) => toTaskReviewPanelFromRecord(record))
        .filter((panel): panel is TaskReviewPanelState => panel !== null);
      setPendingTaskReviewCacheEntry(apiClient, normalizedSessionId, panels);
      return panels;
    })
    .finally(() => {
      setPendingTaskReviewInflight(apiClient, normalizedSessionId, null);
    });

  setPendingTaskReviewInflight(apiClient, normalizedSessionId, inflight);
  return inflight;
};
