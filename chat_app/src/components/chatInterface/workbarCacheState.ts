import type { WorkbarApiClientLike, WorkbarCacheState } from './workbarCache.shared';

const workbarCaches = new WeakMap<WorkbarApiClientLike, WorkbarCacheState>();

export const getOrCreateWorkbarCacheState = (
  apiClient: WorkbarApiClientLike,
): WorkbarCacheState => {
  const existing = workbarCaches.get(apiClient);
  if (existing) {
    return existing;
  }
  const next: WorkbarCacheState = {
    currentTurnCache: new Map(),
    currentTurnInflight: new Map(),
    historyCache: new Map(),
    historyInflight: new Map(),
  };
  workbarCaches.set(apiClient, next);
  return next;
};
