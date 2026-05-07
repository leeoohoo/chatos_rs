import type { TaskReviewPanelState } from '../../lib/store/types';
import { toTaskReviewPanelFromRecord } from './helpers';

interface PendingTaskReviewCacheEntry {
  panels: TaskReviewPanelState[];
  stale: boolean;
}

interface PendingTaskReviewCacheState {
  cache: Map<string, PendingTaskReviewCacheEntry>;
  inflight: Map<string, Promise<TaskReviewPanelState[]>>;
}

interface PendingTaskReviewApiClientLike {
  getPendingTaskReviews: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
}

const pendingTaskReviewCaches = new WeakMap<
  PendingTaskReviewApiClientLike,
  PendingTaskReviewCacheState
>();

const normalizeSessionId = (sessionId: string): string => String(sessionId || '').trim();

const getOrCreatePendingTaskReviewCacheState = (
  apiClient: PendingTaskReviewApiClientLike,
): PendingTaskReviewCacheState => {
  const existing = pendingTaskReviewCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: PendingTaskReviewCacheState = {
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
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreatePendingTaskReviewCacheState(apiClient).cache.get(normalizedSessionId) || null;
};

export const setPendingTaskReviewCacheEntry = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
  panels: TaskReviewPanelState[],
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  getOrCreatePendingTaskReviewCacheState(apiClient).cache.set(normalizedSessionId, {
    panels: [...panels],
    stale: false,
  });
};

export const upsertPendingTaskReviewCachePanel = (
  apiClient: PendingTaskReviewApiClientLike,
  panel: TaskReviewPanelState,
): void => {
  const normalizedSessionId = normalizeSessionId(panel.sessionId);
  if (!normalizedSessionId || !panel.reviewId) {
    return;
  }
  const cacheState = getOrCreatePendingTaskReviewCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  const nextPanels = cached ? [...cached.panels] : [];
  const index = nextPanels.findIndex((item) => item.reviewId === panel.reviewId);
  if (index >= 0) {
    nextPanels[index] = panel;
  } else {
    nextPanels.push(panel);
  }
  cacheState.cache.set(normalizedSessionId, {
    panels: nextPanels,
    stale: false,
  });
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
    ? [normalizeSessionId(sessionId)]
    : Array.from(cacheState.cache.keys());
  for (const normalizedSessionId of candidateSessionIds) {
    if (!normalizedSessionId) {
      continue;
    }
    const cached = cacheState.cache.get(normalizedSessionId);
    if (!cached) {
      continue;
    }
    const nextPanels = cached.panels.filter((panel) => panel.reviewId !== normalizedReviewId);
    if (nextPanels.length === cached.panels.length) {
      continue;
    }
    cacheState.cache.set(normalizedSessionId, {
      panels: nextPanels,
      stale: false,
    });
    break;
  }
};

export const markPendingTaskReviewCacheStale = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreatePendingTaskReviewCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cacheState.cache.set(normalizedSessionId, {
    ...cached,
    stale: true,
  });
};

export const getPendingTaskReviewInflight = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
): Promise<TaskReviewPanelState[]> | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreatePendingTaskReviewCacheState(apiClient).inflight.get(normalizedSessionId) || null;
};

export const setPendingTaskReviewInflight = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
  inflight: Promise<TaskReviewPanelState[]> | null,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreatePendingTaskReviewCacheState(apiClient);
  if (inflight) {
    cacheState.inflight.set(normalizedSessionId, inflight);
    return;
  }
  cacheState.inflight.delete(normalizedSessionId);
};

export const loadPendingTaskReviewPanels = (
  apiClient: PendingTaskReviewApiClientLike,
  sessionId: string,
  options?: { limit?: number; force?: boolean },
): Promise<TaskReviewPanelState[]> => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return Promise.resolve([]);
  }

  const cacheState = getOrCreatePendingTaskReviewCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!options?.force && cached && !cached.stale) {
    return Promise.resolve([...cached.panels]);
  }

  const existingInflight = cacheState.inflight.get(normalizedSessionId);
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
