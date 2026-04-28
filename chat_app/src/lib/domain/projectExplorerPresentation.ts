import type { ChangeKind } from './projectExplorer';

export const CHANGE_KIND_COLOR_CLASS: Record<ChangeKind, string> = {
  create: 'bg-emerald-500',
  edit: 'bg-amber-500',
  delete: 'bg-rose-500',
};

export const CHANGE_KIND_TEXT_CLASS: Record<ChangeKind, string> = {
  create: 'text-emerald-600 dark:text-emerald-400',
  edit: 'text-amber-600 dark:text-amber-400',
  delete: 'text-rose-600 dark:text-rose-400',
};

export const CHANGE_KIND_ROW_CLASS: Record<ChangeKind, string> = {
  create: 'border-l-2 border-emerald-500 bg-emerald-500/10',
  edit: 'border-l-2 border-amber-500 bg-amber-500/10',
  delete: 'border-l-2 border-rose-500 bg-rose-500/10',
};

export const CHANGE_KIND_LABEL: Record<ChangeKind, string> = {
  create: '新增',
  edit: '编辑',
  delete: '删除',
};

export const CHANGE_KIND_PRIORITY: Record<ChangeKind, number> = {
  create: 2,
  edit: 1,
  delete: 3,
};
