import type { TaskWorkbarItem } from '../TaskWorkbar';
import { normalizeSessionScopedId } from './sessionScopedCache';
import {
  normalizeTaskTurnId,
  normalizeTurnId,
  removeTaskFromList,
  type WorkbarApiClientLike,
  type WorkbarCurrentTurnCacheEntry,
  upsertTaskInList,
} from './workbarCache.shared';
import { getOrCreateWorkbarCacheState } from './workbarCacheState';

const buildCurrentTurnCacheKey = (sessionId: string, turnId?: string | null): string => (
  `${normalizeSessionScopedId(sessionId)}::${normalizeTurnId(turnId)}`
);

const peekCurrentTurnCacheEntry = (
  cache: Map<string, WorkbarCurrentTurnCacheEntry>,
  sessionId: string,
  turnId?: string | null,
): WorkbarCurrentTurnCacheEntry | null => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return null;
  }
  return cache.get(cacheKey) || null;
};

const setCurrentTurnCacheEntry = (
  cache: Map<string, WorkbarCurrentTurnCacheEntry>,
  sessionId: string,
  value: { tasks: TaskWorkbarItem[]; turnId: string | null },
): void => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, value.turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return;
  }
  cache.set(cacheKey, {
    ...value,
    stale: false,
  });
};

const updateCurrentTurnCacheEntry = (
  cache: Map<string, WorkbarCurrentTurnCacheEntry>,
  sessionId: string,
  turnId: string | null | undefined,
  updater: (entry: WorkbarCurrentTurnCacheEntry) => WorkbarCurrentTurnCacheEntry,
): void => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return;
  }
  const cached = cache.get(cacheKey);
  if (!cached) {
    return;
  }
  cache.set(cacheKey, updater(cached));
};

const getCurrentTurnInflight = (
  inflightMap: Map<string, Promise<TaskWorkbarItem[]>>,
  sessionId: string,
  turnId?: string | null,
): Promise<TaskWorkbarItem[]> | null => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return null;
  }
  return inflightMap.get(cacheKey) || null;
};

const setCurrentTurnInflight = (
  inflightMap: Map<string, Promise<TaskWorkbarItem[]>>,
  sessionId: string,
  turnId: string | null | undefined,
  inflight: Promise<TaskWorkbarItem[]> | null,
): void => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return;
  }
  if (inflight) {
    inflightMap.set(cacheKey, inflight);
    return;
  }
  inflightMap.delete(cacheKey);
};

export const peekWorkbarCurrentTurnCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  turnId?: string | null,
): WorkbarCurrentTurnCacheEntry | null => (
  peekCurrentTurnCacheEntry(
    getOrCreateWorkbarCacheState(apiClient).currentTurnCache,
    sessionId,
    turnId,
  )
);

export const setWorkbarCurrentTurnCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  value: { tasks: TaskWorkbarItem[]; turnId: string | null },
): void => {
  setCurrentTurnCacheEntry(
    getOrCreateWorkbarCacheState(apiClient).currentTurnCache,
    sessionId,
    value,
  );
};

export const upsertWorkbarCurrentTurnCachedTask = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  task: TaskWorkbarItem,
): boolean => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId) {
    return false;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  const taskTurnId = normalizeTaskTurnId(task.conversationTurnId);
  let changed = false;

  cacheState.currentTurnCache.forEach((entry, key) => {
    if (!key.startsWith(`${normalizedSessionId}::`)) {
      return;
    }

    const hasExisting = entry.tasks.some((item) => item.id === task.id);
    const shouldInsert = !hasExisting
      && entry.turnId !== null
      && normalizeTaskTurnId(entry.turnId) === taskTurnId
      && (entry.tasks.length === 0 || entry.tasks.every(
        (item) => normalizeTaskTurnId(item.conversationTurnId) === taskTurnId,
      ));
    if (!hasExisting && !shouldInsert) {
      return;
    }

    const nextTasks = upsertTaskInList(entry.tasks, task, shouldInsert);
    if (nextTasks === entry.tasks) {
      return;
    }
    cacheState.currentTurnCache.set(key, {
      ...entry,
      tasks: nextTasks,
      stale: false,
    });
    changed = true;
  });
  return changed;
};

export const removeWorkbarCurrentTurnCachedTask = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  taskId: string,
): boolean => {
  const normalizedSessionId = normalizeSessionScopedId(sessionId);
  if (!normalizedSessionId || !taskId) {
    return false;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  let changed = false;

  cacheState.currentTurnCache.forEach((entry, key) => {
    if (!key.startsWith(`${normalizedSessionId}::`)) {
      return;
    }

    const nextTasks = removeTaskFromList(entry.tasks, taskId);
    if (nextTasks === entry.tasks) {
      return;
    }
    cacheState.currentTurnCache.set(key, {
      ...entry,
      tasks: nextTasks,
      stale: false,
    });
    changed = true;
  });
  return changed;
};

export const markWorkbarCurrentTurnStale = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  turnId?: string | null,
): void => {
  updateCurrentTurnCacheEntry(
    getOrCreateWorkbarCacheState(apiClient).currentTurnCache,
    sessionId,
    turnId,
    (cached) => ({
      ...cached,
      stale: true,
    }),
  );
};

export const getWorkbarCurrentTurnInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  turnId?: string | null,
): Promise<TaskWorkbarItem[]> | null => (
  getCurrentTurnInflight(
    getOrCreateWorkbarCacheState(apiClient).currentTurnInflight,
    sessionId,
    turnId,
  )
);

export const setWorkbarCurrentTurnInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  turnId: string | null | undefined,
  inflight: Promise<TaskWorkbarItem[]> | null,
): void => {
  setCurrentTurnInflight(
    getOrCreateWorkbarCacheState(apiClient).currentTurnInflight,
    sessionId,
    turnId,
    inflight,
  );
};
