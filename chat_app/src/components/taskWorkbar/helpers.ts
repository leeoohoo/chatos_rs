import type { RuntimeGuidanceWorkbarItem, TaskWorkbarItem } from './types';

export const statusStyles: Record<TaskWorkbarItem['status'], string> = {
  todo: 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-100',
  doing: 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-200',
  blocked: 'bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-200',
  done: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-200',
};

export const priorityStyles: Record<TaskWorkbarItem['priority'], string> = {
  high: 'text-rose-600 dark:text-rose-300',
  medium: 'text-amber-600 dark:text-amber-300',
  low: 'text-slate-600 dark:text-slate-300',
};

export const statusText: Record<TaskWorkbarItem['status'], string> = {
  todo: '待办',
  doing: '进行中',
  blocked: '阻塞',
  done: '已完成',
};

export const priorityText: Record<TaskWorkbarItem['priority'], string> = {
  high: '高',
  medium: '中',
  low: '低',
};

export const isUnfinishedTask = (task: TaskWorkbarItem): boolean => task.status === 'todo' || task.status === 'doing';

export const isBlockedTask = (task: TaskWorkbarItem): boolean => task.status === 'blocked';

export const isDoneTask = (task: TaskWorkbarItem): boolean => task.status === 'done';

export const selectCurrentWorkbarTask = (items: TaskWorkbarItem[]): TaskWorkbarItem | null => {
  for (const task of items) {
    if (task.status === 'doing') {
      return task;
    }
  }
  for (const task of items) {
    if (task.status === 'todo') {
      return task;
    }
  }
  return null;
};

export const groupWorkbarTasks = (items: TaskWorkbarItem[]) => {
  const current: TaskWorkbarItem[] = [];
  const unfinished: TaskWorkbarItem[] = [];
  const blocked: TaskWorkbarItem[] = [];
  const done: TaskWorkbarItem[] = [];
  const currentTask = selectCurrentWorkbarTask(items);

  for (const task of items) {
    if (currentTask && task.id === currentTask.id) {
      current.push(task);
      continue;
    }
    if (isDoneTask(task)) {
      done.push(task);
      continue;
    }
    if (isBlockedTask(task)) {
      blocked.push(task);
      continue;
    }
    if (isUnfinishedTask(task)) {
      unfinished.push(task);
      continue;
    }
    current.push(task);
  }

  return {
    blocked,
    current,
    done,
    unfinished,
  };
};

export const guidanceStatusStyles: Record<RuntimeGuidanceWorkbarItem['status'], string> = {
  queued: 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-200',
  applied: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-200',
  dropped: 'bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-200',
};

export const guidanceStatusText: Record<RuntimeGuidanceWorkbarItem['status'], string> = {
  queued: '待应用',
  applied: '已应用',
  dropped: '已丢弃',
};

export const sortTasks = (items: TaskWorkbarItem[]) => {
  return [...items].sort((a, b) => {
    const left = Date.parse(a.updatedAt || a.createdAt) || 0;
    const right = Date.parse(b.updatedAt || b.createdAt) || 0;
    return right - left;
  });
};

export const formatGuidanceAppliedTime = (value?: string | null): string => {
  if (!value) {
    return '';
  }
  const time = Date.parse(value);
  if (!Number.isFinite(time)) {
    return '';
  }
  return new Date(time).toLocaleTimeString();
};

export const formatGuidanceItemTime = (item: RuntimeGuidanceWorkbarItem): string => {
  const candidate = item.status === 'applied'
    ? (item.appliedAt || item.createdAt)
    : item.createdAt;
  if (!candidate) {
    return '';
  }
  const parsed = Date.parse(candidate);
  if (!Number.isFinite(parsed)) {
    return '';
  }
  return new Date(parsed).toLocaleTimeString();
};
