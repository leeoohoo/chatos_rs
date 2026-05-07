import type { TaskWorkbarItem } from '../TaskWorkbar';

interface WorkbarCurrentTurnCacheEntry {
  tasks: TaskWorkbarItem[];
  turnId: string | null;
  stale: boolean;
}

interface WorkbarHistoryCacheEntry {
  tasks: TaskWorkbarItem[];
  stale: boolean;
}

interface WorkbarCacheState {
  currentTurnCache: Map<string, WorkbarCurrentTurnCacheEntry>;
  currentTurnInflight: Map<string, Promise<TaskWorkbarItem[]>>;
  historyCache: Map<string, WorkbarHistoryCacheEntry>;
  historyInflight: Map<string, Promise<TaskWorkbarItem[]>>;
}

interface WorkbarApiClientLike {
  getTaskManagerTasks: (
    sessionId: string,
    options?: {
      conversationTurnId?: string;
      includeDone?: boolean;
      limit?: number;
    },
  ) => Promise<unknown[]>;
}

const workbarCaches = new WeakMap<WorkbarApiClientLike, WorkbarCacheState>();

const normalizeSessionId = (sessionId: string): string => String(sessionId || '').trim();
const normalizeTurnId = (turnId: string | null | undefined): string => {
  const normalized = typeof turnId === 'string' ? turnId.trim() : '';
  return normalized || '__latest__';
};
const normalizeTaskTurnId = (turnId: string | null | undefined): string => (
  typeof turnId === 'string' ? turnId.trim() : ''
);
const buildCurrentTurnCacheKey = (sessionId: string, turnId?: string | null): string => (
  `${normalizeSessionId(sessionId)}::${normalizeTurnId(turnId)}`
);

const upsertTaskInList = (
  tasks: TaskWorkbarItem[],
  task: TaskWorkbarItem,
  insertIfMissing: boolean,
): TaskWorkbarItem[] => {
  const index = tasks.findIndex((item) => item.id === task.id);
  if (index >= 0) {
    const nextTasks = [...tasks];
    nextTasks[index] = task;
    return nextTasks;
  }
  if (!insertIfMissing) {
    return tasks;
  }
  return [task, ...tasks];
};

const removeTaskFromList = (
  tasks: TaskWorkbarItem[],
  taskId: string,
): TaskWorkbarItem[] => {
  const nextTasks = tasks.filter((item) => item.id !== taskId);
  return nextTasks.length === tasks.length ? tasks : nextTasks;
};

const getOrCreateWorkbarCacheState = (
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

export const peekWorkbarCurrentTurnCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  turnId?: string | null,
): WorkbarCurrentTurnCacheEntry | null => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return null;
  }
  return getOrCreateWorkbarCacheState(apiClient).currentTurnCache.get(cacheKey) || null;
};

export const setWorkbarCurrentTurnCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  value: { tasks: TaskWorkbarItem[]; turnId: string | null },
): void => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, value.turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return;
  }
  getOrCreateWorkbarCacheState(apiClient).currentTurnCache.set(cacheKey, {
    ...value,
    stale: false,
  });
};

export const upsertWorkbarCurrentTurnCachedTask = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  task: TaskWorkbarItem,
): boolean => {
  const normalizedSessionId = normalizeSessionId(sessionId);
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
  const normalizedSessionId = normalizeSessionId(sessionId);
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
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  const cached = cacheState.currentTurnCache.get(cacheKey);
  if (!cached) {
    return;
  }
  cacheState.currentTurnCache.set(cacheKey, {
    ...cached,
    stale: true,
  });
};

export const getWorkbarCurrentTurnInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  turnId?: string | null,
): Promise<TaskWorkbarItem[]> | null => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return null;
  }
  return getOrCreateWorkbarCacheState(apiClient).currentTurnInflight.get(cacheKey) || null;
};

export const setWorkbarCurrentTurnInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  turnId: string | null | undefined,
  inflight: Promise<TaskWorkbarItem[]> | null,
): void => {
  const cacheKey = buildCurrentTurnCacheKey(sessionId, turnId);
  if (!cacheKey || cacheKey.startsWith('::')) {
    return;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  if (inflight) {
    cacheState.currentTurnInflight.set(cacheKey, inflight);
    return;
  }
  cacheState.currentTurnInflight.delete(cacheKey);
};

export const peekWorkbarHistoryCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
): WorkbarHistoryCacheEntry | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreateWorkbarCacheState(apiClient).historyCache.get(normalizedSessionId) || null;
};

export const setWorkbarHistoryCacheEntry = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  tasks: TaskWorkbarItem[],
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  getOrCreateWorkbarCacheState(apiClient).historyCache.set(normalizedSessionId, {
    tasks,
    stale: false,
  });
};

export const upsertWorkbarHistoryCachedTask = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  task: TaskWorkbarItem,
): boolean => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return false;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  const entry = cacheState.historyCache.get(normalizedSessionId);
  if (!entry) {
    return false;
  }
  const nextTasks = upsertTaskInList(entry.tasks, task, true);
  if (nextTasks === entry.tasks) {
    return false;
  }
  cacheState.historyCache.set(normalizedSessionId, {
    ...entry,
    tasks: nextTasks,
    stale: false,
  });
  return true;
};

export const removeWorkbarHistoryCachedTask = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  taskId: string,
): boolean => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId || !taskId) {
    return false;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  const entry = cacheState.historyCache.get(normalizedSessionId);
  if (!entry) {
    return false;
  }
  const nextTasks = removeTaskFromList(entry.tasks, taskId);
  if (nextTasks === entry.tasks) {
    return false;
  }
  cacheState.historyCache.set(normalizedSessionId, {
    ...entry,
    tasks: nextTasks,
    stale: false,
  });
  return true;
};

export const markWorkbarHistoryStale = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  const cached = cacheState.historyCache.get(normalizedSessionId);
  if (!cached) {
    return;
  }
  cacheState.historyCache.set(normalizedSessionId, {
    ...cached,
    stale: true,
  });
};

export const getWorkbarHistoryInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
): Promise<TaskWorkbarItem[]> | null => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return null;
  }
  return getOrCreateWorkbarCacheState(apiClient).historyInflight.get(normalizedSessionId) || null;
};

export const setWorkbarHistoryInflight = (
  apiClient: WorkbarApiClientLike,
  sessionId: string,
  inflight: Promise<TaskWorkbarItem[]> | null,
): void => {
  const normalizedSessionId = normalizeSessionId(sessionId);
  if (!normalizedSessionId) {
    return;
  }
  const cacheState = getOrCreateWorkbarCacheState(apiClient);
  if (inflight) {
    cacheState.historyInflight.set(normalizedSessionId, inflight);
    return;
  }
  cacheState.historyInflight.delete(normalizedSessionId);
};
