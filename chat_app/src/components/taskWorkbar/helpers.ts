import type { RuntimeGuidanceWorkbarItem, TaskWorkbarItem } from './types';

export const statusStyles: Record<TaskWorkbarItem['status'], string> = {
  pending_confirm: 'bg-amber-100 text-amber-700 dark:bg-amber-900/40 dark:text-amber-200',
  pending_execute: 'bg-slate-200 text-slate-700 dark:bg-slate-700 dark:text-slate-100',
  running: 'bg-blue-100 text-blue-700 dark:bg-blue-900/40 dark:text-blue-200',
  paused: 'bg-violet-100 text-violet-700 dark:bg-violet-900/40 dark:text-violet-200',
  completed: 'bg-emerald-100 text-emerald-700 dark:bg-emerald-900/40 dark:text-emerald-200',
  failed: 'bg-rose-100 text-rose-700 dark:bg-rose-900/40 dark:text-rose-200',
  cancelled: 'bg-slate-300 text-slate-700 dark:bg-slate-800 dark:text-slate-200',
};

export const priorityStyles: Record<TaskWorkbarItem['priority'], string> = {
  high: 'text-rose-600 dark:text-rose-300',
  medium: 'text-amber-600 dark:text-amber-300',
  low: 'text-slate-600 dark:text-slate-300',
};

export const statusText: Record<TaskWorkbarItem['status'], string> = {
  pending_confirm: '待确认',
  pending_execute: '待执行',
  running: '执行中',
  paused: '已暂停',
  completed: '已完成',
  failed: '执行失败',
  cancelled: '已取消',
};

export const priorityText: Record<TaskWorkbarItem['priority'], string> = {
  high: '高',
  medium: '中',
  low: '低',
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
    const left = Date.parse(a.createdAt) || 0;
    const right = Date.parse(b.createdAt) || 0;
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
