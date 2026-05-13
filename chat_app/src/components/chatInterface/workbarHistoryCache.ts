import type { TaskWorkbarItem } from '../TaskWorkbar';
import {
  getSessionScopedInflight,
  markSessionScopedCacheStale,
  normalizeSessionScopedId,
  peekSessionScopedCacheEntry,
  setSessionScopedCacheEntry,
  setSessionScopedInflight,
} from './sessionScopedCache';
import {
  removeTaskFromList,
  type WorkbarApiClientLike,
  type WorkbarHistoryCacheEntry,
  upsertTaskInList,
} from './workbarCache.shared';
import { getOrCreateWorkbarCacheState } from './workbarCacheState';

export const peekWorkbarHistoryCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
): WorkbarHistoryCacheEntry | null => {
  const cached = peekSessionScopedCacheEntry(
    getOrCreateWorkbarCacheState(apiClient).historyCache,
    sessionId,
  );
  return cached
    ? {
      tasks: cached.value,
      stale: cached.stale,
    }
    : null;
};

export const setWorkbarHistoryCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  tasks: TaskWorkbarItem[],
): void => {
  setSessionScopedCacheEntry(
    getOrCreateWorkbarCacheState(apiClient).historyCache,
    sessionId,
    tasks,
  );
};

export const upsertWorkbarHistoryCachedTask = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  task: TaskWorkbarItem,
): boolean => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return false;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  const entry = peekSessionScopedCacheEntry(cacheState.historyCache, normalizedSessionId);
  if (!entry) {
    return false;
  }
  const nextTasks = upsertTaskInList(entry.value, task, true);
  if (nextTasks === entry.value) {
    return false;
  }
  setSessionScopedCacheEntry(cacheState.historyCache, normalizedSessionId, nextTasks);
  return true;
};

export const removeWorkbarHistoryCachedTask = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  taskId: string,
): boolean => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId || !taskId) {
    return false;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  const entry = peekSessionScopedCacheEntry(cacheState.historyCache, normalizedSessionId);
  if (!entry) {
    return false;
  }
  const nextTasks = removeTaskFromList(entry.value, taskId);
  if (nextTasks === entry.value) {
    return false;
  }
  setSessionScopedCacheEntry(cacheState.historyCache, normalizedSessionId, nextTasks);
  return true;
};

export const markWorkbarHistoryStale = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
): void => {
  markSessionScopedCacheStale(
    getOrCreateWorkbarCacheState(apiClient).historyCache,
    sessionId,
  );
};

export const getWorkbarHistoryInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
): Promise<TaskWorkbarItem[]> | null => (
  getSessionScopedInflight(
    getOrCreateWorkbarCacheState(apiClient).historyInflight,
    sessionId,
  )
);

export const setWorkbarHistoryInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  inflight: Promise<TaskWorkbarItem[]> | null,
): void => {
  setSessionScopedInflight(
    getOrCreateWorkbarCacheState(apiClient).historyInflight,
    sessionId,
    inflight,
  );
};
