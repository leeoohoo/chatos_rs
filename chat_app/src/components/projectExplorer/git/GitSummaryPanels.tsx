// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { cn } from '../../../lib/utils';
import type { GitClientInfo } from '../../../types';
import { getGitClientSourceLabel } from './gitBranchButtonShared';
import type { useProjectGit } from './useProjectGit';

export const GitSummaryBlock: React.FC<{
  branchLabel: string;
  changeCount: number;
  summary: NonNullable<ReturnType<typeof useProjectGit>['summary']>;
}> = ({ branchLabel, changeCount, summary }) => {
  const { t } = useI18n();
  const changeBadges = [
    { key: 'staged', label: 'staged', count: summary.changes.staged, className: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700' },
    { key: 'unstaged', label: 'unstaged', count: summary.changes.unstaged, className: 'border-amber-500/30 bg-amber-500/10 text-amber-700' },
    { key: 'untracked', label: 'untracked', count: summary.changes.untracked, className: 'border-sky-500/30 bg-sky-500/10 text-sky-700' },
    { key: 'conflicted', label: 'conflicted', count: summary.changes.conflicted, className: 'border-rose-500/30 bg-rose-500/10 text-rose-700' },
  ].filter((item) => item.count > 0);

  return (
    <div className="mb-2 rounded-md border border-border bg-background p-3 text-xs">
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <div className="truncate font-medium text-foreground">{branchLabel}</div>
          <div className="mt-1 truncate text-muted-foreground">
            {summary.upstream || t('git.upstreamUnset')}
          </div>
        </div>
        <div className="shrink-0 text-right text-muted-foreground">
          <div>↑{summary.ahead} ↓{summary.behind}</div>
          <div className={changeCount > 0 ? 'text-amber-700' : ''}>{t('git.changesCount', { count: changeCount })}</div>
        </div>
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {changeBadges.length > 0 ? changeBadges.map((item) => (
          <span
            key={item.key}
            className={cn('inline-flex items-center rounded-full border px-2 py-1 text-[11px]', item.className)}
          >
            {item.label} {item.count}
          </span>
        )) : (
          <span className="text-muted-foreground">{t('git.cleanSyncHint')}</span>
        )}
      </div>
      <div className="mt-2 text-[11px] text-muted-foreground">
        {t('git.summaryHint')}
      </div>
      {summary.operationState && (
        <div className="mt-2 rounded bg-amber-500/10 px-2 py-1 text-amber-700">
          {t('git.operationState', { state: summary.operationState })}
        </div>
      )}
    </div>
  );
};

export const GitClientInfoBlock: React.FC<{
  clientInfo: GitClientInfo | null;
  loading: boolean;
  onRefresh: () => Promise<void>;
}> = ({ clientInfo, loading, onRefresh }) => {
  const { t } = useI18n();
  const label = clientInfo ? getGitClientSourceLabel(clientInfo.source, t) : 'Git client';
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
          {loading && !clientInfo ? t('git.clientChecking') : `${label}: ${clientInfo?.version || clientInfo?.path || '-'}`}
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
        {t('git.refresh')}
      </button>
    </div>
  );
};
