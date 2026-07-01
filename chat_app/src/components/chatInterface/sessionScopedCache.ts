// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface SessionScopedCacheEntry<T> {
  stale: boolean;
  value: T;
}

export interface SessionScopedCacheState<T> {
  cache: Map<string, SessionScopedCacheEntry<T>>;
  inflight: Map<string, Promise<T>>;
}

export const normalizeSessionScopedId = (sessionId: string): string => (
  String(sessionId || '').trim()
);

export const peekSessionScopedCacheEntry = <T>(
  cache: Map<string, SessionScopedCacheEntry<T>>,
  sessionId: string,
): SessionScopedCacheEntry<T> | null => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return cache.get(normalizedSessionId) || null;
};

export const setSessionScopedCacheEntry = <T>(
  cache: Map<string, SessionScopedCacheEntry<T>>,
  sessionId: string,
  value: T,
): void => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  cache.set(normalizedSessionId, {
    value,
    stale: false,
  });
};

export const updateSessionScopedCacheEntry = <T>(
  cache: Map<string, SessionScopedCacheEntry<T>>,
  sessionId: string,
  updater: (current: T) => T,
): void => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cached = cache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cache.set(normalizedSessionId, {
    value: updater(cached.value),
    stale: false,
  });
};

export const markSessionScopedCacheStale = <T>(
  cache: Map<string, SessionScopedCacheEntry<T>>,
  sessionId: string,
): void => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cached = cache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cache.set(normalizedSessionId, {
    ...cached,
    stale: true,
  });
};

export const getSessionScopedInflight = <T>(
  inflightMap: Map<string, Promise<T>>,
  sessionId: string,
): Promise<T> | null => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return inflightMap.get(normalizedSessionId) || null;
};

export const setSessionScopedInflight = <T>(
  inflightMap: Map<string, Promise<T>>,
  sessionId: string,
  inflight: Promise<T> | null,
): void => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  if (inflight) {
    inflightMap.set(normalizedSessionId, inflight);
    return;
  }
  inflightMap.delete(normalizedSessionId);
};
