// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { GitCompareResult } from '../../../types';
import { getGitStatusLabel } from './gitBranchButtonShared';

const CommitList: React.FC<{
  title: string;
  commits: GitCompareResult['commits'];
}> = ({ title, commits }) => {
  const { t } = useI18n();

  return (
    <div className="overflow-hidden rounded border border-border">
      <div className="border-b border-border px-2 py-1 text-[11px] font-medium text-muted-foreground">
        {title}
      </div>
      {commits.length === 0 ? (
        <div className="px-2 py-2 text-[11px] text-muted-foreground">{t('git.compare.none')}</div>
      ) : commits.slice(0, 6).map((commit) => (
        <div key={`${title}:${commit.hash}`} className="border-b border-border px-2 py-1.5 text-[11px] last:border-b-0">
          <span className="font-mono text-muted-foreground">{commit.hash}</span>
          <span className="ml-2 text-foreground">{commit.subject || '(no subject)'}</span>
        </div>
      ))}
      {commits.length > 6 && (
        <div className="px-2 py-1.5 text-[11px] text-muted-foreground">
          {t('git.compare.moreCommits', { count: commits.length - 6 })}
        </div>
      )}
    </div>
  );
};

export const ComparePanel: React.FC<{
  compareResult: GitCompareResult | null;
  loadingCompare: boolean;
  loadingDiff: boolean;
  onLoadFileDiff: (path: string, target?: string, staged?: boolean) => Promise<void>;
  onClear: () => void;
}> = ({
  compareResult,
  loadingCompare,
  loadingDiff,
  onLoadFileDiff,
  onClear,
}) => {
  const { t } = useI18n();

  if (loadingCompare && !compareResult) {
    return (
      <div className="mb-3 flex items-center justify-between gap-3 rounded border border-border bg-background px-3 py-2 text-xs text-muted-foreground">
        <span>{t('git.compare.loading')}</span>
        <button
          type="button"
          onClick={onClear}
          className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent"
        >
          {t('git.compare.back')}
        </button>
      </div>
    );
  }
  if (!compareResult) return null;

  const currentCommits = compareResult.commits.filter((commit) => commit.side === 'current');
  const targetCommits = compareResult.commits.filter((commit) => commit.side === 'target');

  return (
    <div className="mb-3 overflow-hidden rounded-md border border-border bg-background">
      <div className="flex items-center justify-between gap-3 border-b border-border px-3 py-2">
        <div className="flex min-w-0 items-center gap-2 text-xs">
          <button
            type="button"
            onClick={onClear}
            className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent"
          >
            {t('git.compare.back')}
          </button>
          <div className="min-w-0">
            <div className="truncate font-medium text-foreground">
              {compareResult.current} ↔ {compareResult.target}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {t('git.compare.summary', {
                files: compareResult.files.length,
                targetCommits: targetCommits.length,
                currentCommits: currentCommits.length,
              })}
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={onClear}
          className="h-7 shrink-0 rounded border border-border px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          {t('git.compare.clear')}
        </button>
      </div>

      <div className="grid gap-2 p-2 md:grid-cols-[minmax(0,1fr)_220px]">
        <div className="overflow-hidden rounded border border-border">
          {compareResult.files.length === 0 ? (
            <div className="px-3 py-2 text-xs text-muted-foreground">{t('git.compare.noFileDiffs')}</div>
          ) : compareResult.files.map((file) => (
            <button
              key={`${file.status}:${file.oldPath || ''}:${file.path}`}
              type="button"
              onClick={() => { void onLoadFileDiff(file.path, compareResult.target); }}
              disabled={loadingDiff}
              className="flex w-full items-center gap-2 border-b border-border px-3 py-2 text-left text-xs last:border-b-0 hover:bg-accent disabled:opacity-50"
            >
              <span className="shrink-0 rounded bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                {getGitStatusLabel(file.status, t)}
              </span>
              <span className="min-w-0 flex-1 truncate font-mono text-[11px]" title={file.path}>
                {file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
              </span>
            </button>
          ))}
        </div>

        <div className="space-y-2">
          <CommitList title={t('git.compare.targetOnly')} commits={targetCommits} />
          <CommitList title={t('git.compare.currentOnly')} commits={currentCommits} />
        </div>
      </div>
    </div>
  );
};
