import React from 'react';

import type { ProjectChangeSummary } from '../../../types';
import { cn } from '../../../lib/utils';

export const ProjectTreeChangeCounters: React.FC<{
  changeSummary: ProjectChangeSummary;
}> = ({ changeSummary }) => (
  <div className="flex items-center gap-3 text-[11px] text-muted-foreground">
    <span className="inline-flex items-center gap-1">
      <span className="inline-block h-2 w-2 rounded-full bg-emerald-500" />
      新增 {changeSummary.counts.create}
    </span>
    <span className="inline-flex items-center gap-1">
      <span className="inline-block h-2 w-2 rounded-full bg-amber-500" />
      编辑 {changeSummary.counts.edit}
    </span>
    <span className="inline-flex items-center gap-1">
      <span className="inline-block h-2 w-2 rounded-full bg-rose-500" />
      删除 {changeSummary.counts.delete}
    </span>
  </div>
);

export const ProjectTreeHeaderActions: React.FC<{
  actionLoading: boolean;
  actionReloadPath: string | null;
  canConfirmCurrent: boolean;
  showOnlyChanged: boolean;
  totalChangeCount: number;
  onCreateDirectoryAtRoot: () => void;
  onCreateFileAtRoot: () => void;
  onRefresh: () => void;
  onConfirmCurrent: () => void;
  onConfirmAll: () => void;
  onToggleShowOnlyChanged: () => void;
}> = ({
  actionLoading,
  actionReloadPath,
  canConfirmCurrent,
  showOnlyChanged,
  totalChangeCount,
  onCreateDirectoryAtRoot,
  onCreateFileAtRoot,
  onRefresh,
  onConfirmCurrent,
  onConfirmAll,
  onToggleShowOnlyChanged,
}) => (
  <div className="flex flex-wrap gap-1">
    <button
      type="button"
      onClick={onCreateDirectoryAtRoot}
      disabled={actionLoading}
      className="rounded border border-blue-500/40 px-2 py-1 text-[11px] text-blue-700 hover:bg-blue-500/10 disabled:cursor-not-allowed disabled:opacity-50"
    >
      根目录新建目录
    </button>
    <button
      type="button"
      onClick={onCreateFileAtRoot}
      disabled={actionLoading}
      className="rounded border border-blue-500/40 px-2 py-1 text-[11px] text-blue-700 hover:bg-blue-500/10 disabled:cursor-not-allowed disabled:opacity-50"
    >
      根目录新建文件
    </button>
    <button
      type="button"
      onClick={onRefresh}
      disabled={!actionReloadPath || actionLoading}
      className="rounded border border-border px-2 py-1 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
    >
      刷新
    </button>
    <button
      type="button"
      onClick={onConfirmCurrent}
      disabled={!canConfirmCurrent || actionLoading}
      className="rounded border border-amber-500/40 px-2 py-1 text-[11px] text-amber-700 hover:bg-amber-500/10 disabled:cursor-not-allowed disabled:opacity-50"
    >
      确认当前项
    </button>
    <button
      type="button"
      onClick={onConfirmAll}
      disabled={totalChangeCount <= 0 || actionLoading}
      className="rounded border border-emerald-500/40 px-2 py-1 text-[11px] text-emerald-700 hover:bg-emerald-500/10 disabled:cursor-not-allowed disabled:opacity-50"
    >
      确认全部变更
    </button>
    <button
      type="button"
      onClick={onToggleShowOnlyChanged}
      className={cn(
        'rounded border px-2 py-1 text-[11px] disabled:cursor-not-allowed disabled:opacity-50',
        showOnlyChanged
          ? 'border-emerald-500/50 bg-emerald-500/10 text-emerald-700 hover:bg-emerald-500/20'
          : 'border-border hover:bg-accent',
      )}
    >
      {showOnlyChanged ? '显示全部' : '仅看变更'}
    </button>
  </div>
);

export const ProjectTreeHeaderMessages: React.FC<{
  loadingSummary: boolean;
  summaryError: string | null;
  actionMessage: string | null;
  actionError: string | null;
}> = ({
  loadingSummary,
  summaryError,
  actionMessage,
  actionError,
}) => (
  <>
    {loadingSummary && (
      <div className="text-[11px] text-muted-foreground">正在加载变更标记...</div>
    )}
    {summaryError && (
      <div className="truncate text-[11px] text-destructive" title={summaryError}>
        {summaryError}
      </div>
    )}
    {actionMessage && (
      <div className="truncate text-[11px] text-emerald-600" title={actionMessage}>
        {actionMessage}
      </div>
    )}
    {actionError && (
      <div className="truncate text-[11px] text-destructive" title={actionError}>
        {actionError}
      </div>
    )}
  </>
);
