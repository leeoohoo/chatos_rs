import type ApiClient from '../../api/client';
import { ApiRequestError } from '../../api/client/shared';
import type { Terminal } from '../../../types';
import { normalizeTerminal } from '../../domain/terminals';

interface TerminalsListCacheEntry {
  terminals: Terminal[];
  stale: boolean;
}

interface TerminalsDetailCacheEntry {
  terminal: Terminal;
  stale: boolean;
}

interface TerminalsClientCacheState {
  listCache: Map<string, TerminalsListCacheEntry>;
  listInflight: Map<string, Promise<Terminal[]>>;
  detailCache: Map<string, TerminalsDetailCacheEntry>;
  detailInflight: Map<string, Promise<Terminal | null>>;
}

const terminalsClientCaches = new WeakMap<ApiClient, TerminalsClientCacheState>();

const normalizeUserId = (userId: string): string => String(userId || '').trim();
const normalizeTerminalId = (terminalId: string): string => String(terminalId || '').trim();

const buildTerminalsListCacheKey = (userId?: string | null): string => {
  const normalizedUserId = normalizeUserId(String(userId || ''));
  return normalizedUserId || '__current_user__';
};

const getOrCreateClientCacheState = (apiClient: ApiClient): TerminalsClientCacheState => {
  const existing = terminalsClientCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: TerminalsClientCacheState = {
    listCache: new Map(),
    listInflight: new Map(),
    detailCache: new Map(),
    detailInflight: new Map(),
  };
  terminalsClientCaches.set(apiClient, next);
  return next;
};

export const upsertTerminal = (
  terminals: Terminal[],
  terminal: Terminal,
): Terminal[] => {
  const normalizedTerminalId = normalizeTerminalId(terminal.id);
  if (!normalizedTerminalId) {
    return terminals;
  }
  const index = terminals.findIndex((item) => item.id === normalizedTerminalId);
  if (index === -1) {
    return [terminal, ...terminals];
  }
  const next = [...terminals];
  next[index] = terminal;
  return next;
};

export const removeTerminal = (
  terminals: Terminal[],
  terminalId: string,
): Terminal[] => {
  const normalizedTerminalId = normalizeTerminalId(terminalId);
  if (!normalizedTerminalId) {
    return terminals;
  }
  return terminals.filter((item) => item.id !== normalizedTerminalId);
};

export const syncTerminalDetailCache = (
  apiClient: ApiClient,
  terminal: Terminal,
): void => {
  const normalizedTerminalId = normalizeTerminalId(terminal.id);
  if (!normalizedTerminalId) {
    return;
  }
  getOrCreateClientCacheState(apiClient).detailCache.set(normalizedTerminalId, {
    terminal,
    stale: false,
  });
};

export const syncTerminalListCaches = (
  apiClient: ApiClient,
  updater: (terminals: Terminal[]) => Terminal[],
): void => {
  const cacheState = getOrCreateClientCacheState(apiClient);
  cacheState.listCache.forEach((entry, key) => {
    cacheState.listCache.set(key, {
      terminals: updater(entry.terminals),
      stale: false,
    });
  });
};

export const syncLoadedTerminals = (
  apiClient: ApiClient,
  userId: string | null | undefined,
  terminals: Terminal[],
): void => {
  const cacheState = getOrCreateClientCacheState(apiClient);
  cacheState.listCache.set(buildTerminalsListCacheKey(userId), {
    terminals,
    stale: false,
  });
  terminals.forEach((terminal) => {
    syncTerminalDetailCache(apiClient, terminal);
  });
};

export const upsertTerminalCaches = (
  apiClient: ApiClient,
  terminal: Terminal,
): void => {
  syncTerminalDetailCache(apiClient, terminal);
  syncTerminalListCaches(apiClient, (terminals) => upsertTerminal(terminals, terminal));
};

export const removeTerminalCaches = (
  apiClient: ApiClient,
  terminalId: string,
): void => {
  const normalizedTerminalId = normalizeTerminalId(terminalId);
  if (!normalizedTerminalId) {
    return;
  }
  const cacheState = getOrCreateClientCacheState(apiClient);
  cacheState.detailCache.delete(normalizedTerminalId);
  cacheState.detailInflight.delete(normalizedTerminalId);
  syncTerminalListCaches(apiClient, (terminals) => removeTerminal(terminals, normalizedTerminalId));
};

export const markTerminalCachesStale = (
  apiClient: ApiClient,
  options?: { userId?: string | null; terminalId?: string | null },
): void => {
  const cacheState = getOrCreateClientCacheState(apiClient);
  const normalizedUserId = normalizeUserId(String(options?.userId || ''));
  const normalizedTerminalId = normalizeTerminalId(String(options?.terminalId || ''));

  if (normalizedUserId) {
    const cacheKey = buildTerminalsListCacheKey(normalizedUserId);
    const cached = cacheState.listCache.get(cacheKey);
    if (cached) {
      cacheState.listCache.set(cacheKey, {
        ...cached,
        stale: true,
      });
    }
  } else {
    cacheState.listCache.forEach((entry, key) => {
      cacheState.listCache.set(key, {
        ...entry,
        stale: true,
      });
    });
  }

  if (normalizedTerminalId) {
    const cached = cacheState.detailCache.get(normalizedTerminalId);
    if (cached) {
      cacheState.detailCache.set(normalizedTerminalId, {
        ...cached,
        stale: true,
      });
    }
  }
};

export const peekTerminalListCacheEntry = (
  apiClient: ApiClient,
  userId?: string | null,
): TerminalsListCacheEntry | null => {
  return getOrCreateClientCacheState(apiClient).listCache.get(buildTerminalsListCacheKey(userId)) || null;
};

export const getTerminalListInflight = (
  apiClient: ApiClient,
  userId?: string | null,
): Promise<Terminal[]> | null => {
  return getOrCreateClientCacheState(apiClient).listInflight.get(buildTerminalsListCacheKey(userId)) || null;
};

export const setTerminalListInflight = (
  apiClient: ApiClient,
  userId: string | null | undefined,
  inflight: Promise<Terminal[]> | null,
): void => {
  const cacheKey = buildTerminalsListCacheKey(userId);
  const cacheState = getOrCreateClientCacheState(apiClient);
  if (inflight) {
    cacheState.listInflight.set(cacheKey, inflight);
    return;
  }
  cacheState.listInflight.delete(cacheKey);
};

export const peekTerminalDetailCacheEntry = (
  apiClient: ApiClient,
  terminalId: string,
): TerminalsDetailCacheEntry | null => {
  const normalizedTerminalId = normalizeTerminalId(terminalId);
  if (!normalizedTerminalId) {
    return null;
  }
  return getOrCreateClientCacheState(apiClient).detailCache.get(normalizedTerminalId) || null;
};

export const getTerminalDetailInflight = (
  apiClient: ApiClient,
  terminalId: string,
): Promise<Terminal | null> | null => {
  const normalizedTerminalId = normalizeTerminalId(terminalId);
  if (!normalizedTerminalId) {
    return null;
  }
  return getOrCreateClientCacheState(apiClient).detailInflight.get(normalizedTerminalId) || null;
};

export const setTerminalDetailInflight = (
  apiClient: ApiClient,
  terminalId: string,
  inflight: Promise<Terminal | null> | null,
): void => {
  const normalizedTerminalId = normalizeTerminalId(terminalId);
  if (!normalizedTerminalId) {
    return;
  }
  const cacheState = getOrCreateClientCacheState(apiClient);
  if (inflight) {
    cacheState.detailInflight.set(normalizedTerminalId, inflight);
    return;
  }
  cacheState.detailInflight.delete(normalizedTerminalId);
};

export const loadTerminalDetailSnapshot = async (
  apiClient: ApiClient,
  terminalId: string,
  options?: { force?: boolean },
): Promise<Terminal | null> => {
  const normalizedTerminalId = normalizeTerminalId(terminalId);
  if (!normalizedTerminalId) {
    return null;
  }

  const cacheState = getOrCreateClientCacheState(apiClient);
  const cached = cacheState.detailCache.get(normalizedTerminalId);
  if (!options?.force && cached && !cached.stale) {
    return cached.terminal;
  }

  let inflight = cacheState.detailInflight.get(normalizedTerminalId);
  if (!inflight) {
    inflight = apiClient.getTerminal(normalizedTerminalId)
      .then((payload) => normalizeTerminal(payload))
      .then((terminal) => {
        upsertTerminalCaches(apiClient, terminal);
        return terminal;
      })
      .catch((error) => {
        if (error instanceof ApiRequestError && error.status === 404) {
          removeTerminalCaches(apiClient, normalizedTerminalId);
          return null;
        }
        throw error;
      })
      .finally(() => {
        cacheState.detailInflight.delete(normalizedTerminalId);
      });
    cacheState.detailInflight.set(normalizedTerminalId, inflight);
  }

  return inflight;
};

export const loadTerminalsSnapshot = async (
  apiClient: ApiClient,
  userId?: string | null,
  options?: { force?: boolean },
): Promise<Terminal[]> => {
  const cacheKey = buildTerminalsListCacheKey(userId);
  const cacheState = getOrCreateClientCacheState(apiClient);
  const cached = cacheState.listCache.get(cacheKey);
  if (!options?.force && cached && !cached.stale) {
    return cached.terminals;
  }

  let inflight = cacheState.listInflight.get(cacheKey);
  if (!inflight) {
    inflight = apiClient.listTerminals(userId || undefined)
      .then((list) => {
        const terminals = Array.isArray(list) ? list.map(normalizeTerminal) : [];
        syncLoadedTerminals(apiClient, userId, terminals);
        return terminals;
      })
      .finally(() => {
        cacheState.listInflight.delete(cacheKey);
      });
    cacheState.listInflight.set(cacheKey, inflight);
  }

  return inflight;
};
