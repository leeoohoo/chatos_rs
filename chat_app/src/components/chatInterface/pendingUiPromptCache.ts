import type { UiPromptPanelState } from '../../lib/store/types';
import { toUiPromptPanelFromRecord } from './panelTransforms';
import {
  getSessionScopedInflight,
  markSessionScopedCacheStale,
  normalizeSessionScopedId,
  peekSessionScopedCacheEntry,
  setSessionScopedCacheEntry,
  setSessionScopedInflight,
  type SessionScopedCacheState,
} from './sessionScopedCache';

interface PendingUiPromptCacheEntry {
  panels: UiPromptPanelState[];
  stale: boolean;
}

interface PendingUiPromptApiClientLike {
  getPendingUiPrompts: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
}

const pendingUiPromptCaches = new WeakMap<
  PendingUiPromptApiClientLike,
  SessionScopedCacheState<UiPromptPanelState[]>
>();

const getOrCreatePendingUiPromptCacheState = (
  apiClient: PendingUiPromptApiClientLike,
): SessionScopedCacheState<UiPromptPanelState[]> => {
  const existing = pendingUiPromptCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: SessionScopedCacheState<UiPromptPanelState[]> = {
    cache: new Map(),
    inflight: new Map(),
  };
  pendingUiPromptCaches.set(apiClient, next);
  return next;
};

export const peekPendingUiPromptCacheEntry = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
): PendingUiPromptCacheEntry | null => {
  const cached = peekSessionScopedCacheEntry(
    getOrCreatePendingUiPromptCacheState(apiClient).cache,
    sessionId,
  );
  return cached
    ? {
      panels: cached.value,
      stale: cached.stale,
    }
    : null;
};

export const setPendingUiPromptCacheEntry = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
  panels: UiPromptPanelState[],
): void => {
  setSessionScopedCacheEntry(
    getOrCreatePendingUiPromptCacheState(apiClient).cache,
    sessionId,
    [...panels],
  );
};

export const upsertPendingUiPromptCachePanel = (
  apiClient: PendingUiPromptApiClientLike,
  panel: UiPromptPanelState,
): void => {
  const normalizedSessionId = normalizeSessionScopedId(panel.sessionId);
  if (!normalizedSessionId || !panel.promptId) {
    return;
  }
  const cacheState = getOrCreatePendingUiPromptCacheState(apiClient);
  const cached = peekSessionScopedCacheEntry(cacheState.cache, normalizedSessionId);
  const nextPanels = cached ? [...cached.value] : [];
  const index = nextPanels.findIndex((item) => item.promptId === panel.promptId);
  if (index >= 0) {
    nextPanels[index] = panel;
  } else {
    nextPanels.push(panel);
  }
  setSessionScopedCacheEntry(cacheState.cache, normalizedSessionId, nextPanels);
};

export const removePendingUiPromptCachePanel = (
  apiClient: PendingUiPromptApiClientLike,
  promptId: string,
  sessionId?: string,
): void => {
  const normalizedPromptId = String(promptId || '').trim();
  if (!normalizedPromptId) {
    return;
  }
  const cacheState = getOrCreatePendingUiPromptCacheState(apiClient);
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
    const nextPanels = cached.value.filter((panel) => panel.promptId !== normalizedPromptId);
    if (nextPanels.length === cached.value.length) {
      continue;
    }
    setSessionScopedCacheEntry(cacheState.cache, normalizedSessionId, nextPanels);
    break;
  }
};

export const markPendingUiPromptCacheStale = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
): void => {
  markSessionScopedCacheStale(
    getOrCreatePendingUiPromptCacheState(apiClient).cache,
    sessionId,
  );
};

export const getPendingUiPromptInflight = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
): Promise<UiPromptPanelState[]> | null => {
  return getSessionScopedInflight(
    getOrCreatePendingUiPromptCacheState(apiClient).inflight,
    sessionId,
  );
};

export const setPendingUiPromptInflight = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
  inflight: Promise<UiPromptPanelState[]> | null,
): void => {
  setSessionScopedInflight(
    getOrCreatePendingUiPromptCacheState(apiClient).inflight,
    sessionId,
    inflight,
  );
};

export const loadPendingUiPromptPanels = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
  options?: { limit?: number; force?: boolean },
): Promise<UiPromptPanelState[]> => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return Promise.resolve([]);
  }

  const cacheState = getOrCreatePendingUiPromptCacheState(apiClient);
  const cached = peekSessionScopedCacheEntry(cacheState.cache, normalizedSessionId);
  if (!options?.force && cached && !cached.stale) {
    return Promise.resolve([...cached.value]);
  }

  const existingInflight = getSessionScopedInflight(cacheState.inflight, normalizedSessionId);
  if (existingInflight) {
    return existingInflight;
  }

  const inflight = apiClient
    .getPendingUiPrompts(normalizedSessionId, { limit: options?.limit })
    .then((records) => {
      if (!Array.isArray(records)) {
        return [] as UiPromptPanelState[];
      }
      const panels = records
        .map((record) => toUiPromptPanelFromRecord(record))
        .filter((panel): panel is UiPromptPanelState => panel !== null);
      setPendingUiPromptCacheEntry(apiClient, normalizedSessionId, panels);
      return panels;
    })
    .finally(() => {
      setPendingUiPromptInflight(apiClient, normalizedSessionId, null);
    });

  setPendingUiPromptInflight(apiClient, normalizedSessionId, inflight);
  return inflight;
};
