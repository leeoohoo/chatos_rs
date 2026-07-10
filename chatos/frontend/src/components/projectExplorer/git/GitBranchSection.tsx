// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import { cn } from '../../../lib/utils';
import type { GitBranchInfo } from '../../../types';

export const BranchSection: React.FC<{
  title: string;
  branches: GitBranchInfo[];
  loading: boolean;
  actionLoading: boolean;
  loadingCompare: boolean;
  operationState?: string | null;
  readOnly?: boolean;
  onCheckout: (branch: GitBranchInfo) => Promise<void>;
  onMerge: (branch: GitBranchInfo) => Promise<void>;
  onCompare: (branch: GitBranchInfo) => Promise<void>;
}> = ({
  title,
  branches,
  loading,
  actionLoading,
  loadingCompare,
  operationState,
  readOnly = false,
  onCheckout,
  onMerge,
  onCompare,
}) => {
  const { t } = useI18n();

  return (
    <div className="mb-3">
      <div className="mb-1 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        {title}
      </div>
      <div className="overflow-hidden rounded border border-border">
        {loading ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">{t('git.loading')}</div>
        ) : branches.length === 0 ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">{t('git.branch.noMatches')}</div>
        ) : branches.map((branch) => (
          <div
            key={`${title}:${branch.name}`}
            className={cn(
              'flex items-center justify-between gap-3 border-b border-border px-3 py-2 text-xs last:border-b-0',
              branch.current && 'bg-accent/70',
            )}
          >
            <span className="min-w-0">
              <span className="block truncate font-medium text-foreground">
                {branch.current ? '✓ ' : ''}{branch.shortName || branch.name}
              </span>
              <span className="block truncate text-[11px] text-muted-foreground">
                {branch.upstream || branch.trackedBy || branch.lastCommitSubject || branch.name}
              </span>
            </span>
            {(branch.ahead > 0 || branch.behind > 0) && (
              <span className="shrink-0 text-[11px] text-muted-foreground">
                ↑{branch.ahead} ↓{branch.behind}
              </span>
            )}
            {!readOnly && (
              <span className="flex shrink-0 items-center gap-1">
                <button
                  type="button"
                  disabled={loadingCompare || branch.current}
                  onClick={() => { void onCompare(branch); }}
                  className="h-7 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Compare
                </button>
                <button
                  type="button"
                  disabled={actionLoading || branch.current || Boolean(operationState)}
                  title={operationState ? t('git.branch.mergeDisabledTitle', { state: operationState }) : undefined}
                  onClick={() => { void onMerge(branch); }}
                  className="h-7 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Merge
                </button>
                <button
                  type="button"
                  disabled={actionLoading || branch.current}
                  onClick={() => { void onCheckout(branch); }}
                  className="h-7 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Checkout
                </button>
              </span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};
