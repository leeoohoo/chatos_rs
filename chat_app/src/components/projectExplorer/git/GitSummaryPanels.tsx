import React from 'react';

import { cn } from '../../../lib/utils';
import type { GitClientInfo } from '../../../types';
import { gitClientSourceLabel } from './gitBranchButtonShared';
import type { useProjectGit } from './useProjectGit';

export const GitSummaryBlock: React.FC<{
  branchLabel: string;
  changeCount: number;
  summary: NonNullable<ReturnType<typeof useProjectGit>['summary']>;
}> = ({ branchLabel, changeCount, summary }) => (
  <div className="mb-2 rounded-md border border-border bg-background p-3 text-xs">
    <div className="flex items-center justify-between gap-3">
      <div className="min-w-0">
        <div className="truncate font-medium text-foreground">{branchLabel}</div>
        <div className="mt-1 truncate text-muted-foreground">
          {summary.upstream || '未设置 upstream'}
        </div>
      </div>
      <div className="shrink-0 text-right text-muted-foreground">
        <div>↑{summary.ahead} ↓{summary.behind}</div>
        <div className={changeCount > 0 ? 'text-amber-700' : ''}>{changeCount} 个变更</div>
      </div>
    </div>
    {summary.operationState && (
      <div className="mt-2 rounded bg-amber-500/10 px-2 py-1 text-amber-700">
        当前 Git 操作状态：{summary.operationState}
      </div>
    )}
  </div>
);

export const GitClientInfoBlock: React.FC<{
  clientInfo: GitClientInfo | null;
  loading: boolean;
  onRefresh: () => Promise<void>;
}> = ({ clientInfo, loading, onRefresh }) => {
  const label = clientInfo ? gitClientSourceLabel[clientInfo.source] || clientInfo.source : 'Git client';
  return (
    <div className={cn(
      'mb-2 flex items-center justify-between gap-3 rounded border px-3 py-2 text-xs',
      clientInfo?.available === false
        ? 'border-destructive/30 bg-destructive/5 text-destructive'
        : 'border-border bg-background text-muted-foreground',
    )}
    >
      <div className="min-w-0">
        <div className="truncate">
          {loading && !clientInfo ? 'Git client 检查中...' : `${label}: ${clientInfo?.version || clientInfo?.path || '-'}`}
        </div>
        {clientInfo?.error && (
          <div className="mt-1 line-clamp-2 text-[11px]">
            {clientInfo.error}
          </div>
        )}
      </div>
      <button
        type="button"
        onClick={() => { void onRefresh(); }}
        disabled={loading}
        className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:opacity-50"
      >
        刷新
      </button>
    </div>
  );
};
