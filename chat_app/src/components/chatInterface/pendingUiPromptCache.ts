import type { UiPromptPanelState } from '../../lib/store/types';
import { toUiPromptPanelFromRecord } from './helpers';

interface PendingUiPromptCacheEntry {
  panels: UiPromptPanelState[];
  stale: boolean;
}

interface PendingUiPromptCacheState {
  cache: Map<string, PendingUiPromptCacheEntry>;
  inflight: Map<string, Promise<UiPromptPanelState[]>>;
}

interface PendingUiPromptApiClientLike {
  getPendingUiPrompts: (
    sessionId: string,
    options?: { limit?: number },
  ) => Promise<unknown[]>;
}

const pendingUiPromptCaches = new WeakMap<PendingUiPromptApiClientLike, PendingUiPromptCacheState>();

const normalizeSessionId = (sessionId: string): string => String(sessionId || '').trim();

const getOrCreatePendingUiPromptCacheState = (
  apiClient: PendingUiPromptApiClientLike,
): PendingUiPromptCacheState => {
  const existing = pendingUiPromptCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: PendingUiPromptCacheState = {
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
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreatePendingUiPromptCacheState(apiClient).cache.get(normalizedSessionId) || null;
};

export const setPendingUiPromptCacheEntry = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
  panels: UiPromptPanelState[],
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  getOrCreatePendingUiPromptCacheState(apiClient).cache.set(normalizedSessionId, {
    panels: [...panels],
    stale: false,
  });
};

export const upsertPendingUiPromptCachePanel = (
  apiClient: PendingUiPromptApiClientLike,
  panel: UiPromptPanelState,
): void => {
  const normalizedSessionId = normalizeSessionId(panel.sessionId);
  if (!normalizedSessionId || !panel.promptId) {
    return;
  }
  const cacheState = getOrCreatePendingUiPromptCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  const nextPanels = cached ? [...cached.panels] : [];
  const index = nextPanels.findIndex((item) => item.promptId === panel.promptId);
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
    const nextPanels = cached.panels.filter((panel) => panel.promptId !== normalizedPromptId);
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

export const markPendingUiPromptCacheStale = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreatePendingUiPromptCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cacheState.cache.set(normalizedSessionId, {
    ...cached,
    stale: true,
  });
};

export const getPendingUiPromptInflight = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
): Promise<UiPromptPanelState[]> | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreatePendingUiPromptCacheState(apiClient).inflight.get(normalizedSessionId) || null;
};

export const setPendingUiPromptInflight = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
  inflight: Promise<UiPromptPanelState[]> | null,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreatePendingUiPromptCacheState(apiClient);
  if (inflight) {
    cacheState.inflight.set(normalizedSessionId, inflight);
    return;
  }
  cacheState.inflight.delete(normalizedSessionId);
};

export const loadPendingUiPromptPanels = (
  apiClient: PendingUiPromptApiClientLike,
  sessionId: string,
  options?: { limit?: number; force?: boolean },
): Promise<UiPromptPanelState[]> => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return Promise.resolve([]);
  }

  const cacheState = getOrCreatePendingUiPromptCacheState(apiClient);
  const cached = cacheState.cache.get(normalizedSessionId);
  if (!options?.force && cached && !cached.stale) {
    return Promise.resolve([...cached.panels]);
  }

  const existingInflight = cacheState.inflight.get(normalizedSessionId);
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
