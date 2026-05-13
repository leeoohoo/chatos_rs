import type { TaskWorkbarItem } from '../TaskWorkbar';
import type { SessionScopedCacheState } from './sessionScopedCache';

export interface WorkbarCurrentTurnCacheEntry {
  tasks: TaskWorkbarItem[];
  turnId: string | null;
  stale: boolean;
}

export interface WorkbarHistoryCacheEntry {
  tasks: TaskWorkbarItem[];
  stale: boolean;
}

export interface WorkbarCacheState {
  currentTurnCache: Map<string, WorkbarCurrentTurnCacheEntry>;
  currentTurnInflight: Map<string, Promise<TaskWorkbarItem[]>>;
  historyCache: SessionScopedCacheState<TaskWorkbarItem[]>['cache'];
  historyInflight: SessionScopedCacheState<TaskWorkbarItem[]>['inflight'];
}

export interface WorkbarApiClientLike {
  getTaskManagerTasks: (
    sessionId: string,
    options?: {
      conversationTurnId?: string;
      includeDone?: boolean;
      limit?: number;
    },
  ) => Promise<unknown[]>;
}

export const normalizeTurnId = (turnId: string | null | undefined): string => {
  const normalized = typeof turnId === 'string' ? turnId.trim() : '';
  return normalized || '__latest__';
};

export const normalizeTaskTurnId = (turnId: string | null | undefined): string => (
  typeof turnId === 'string' ? turnId.trim() : ''
);

export const upsertTaskInList = (
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

export const removeTaskFromList = (
  tasks: TaskWorkbarItem[],
  taskId: string,
): TaskWorkbarItem[] => {
  const nextTasks = tasks.filter((item) => item.id !== taskId);
  return nextTasks.length === tasks.length ? tasks : nextTasks;
};
