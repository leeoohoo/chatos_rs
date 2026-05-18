import React from 'react';

import type { GitStatusFile } from '../../../types';
import { statusLabel, statusTitle } from './gitBranchButtonShared';

export const StatusSection: React.FC<{
  files: GitStatusFile[];
  loading: boolean;
  loadingDiff: boolean;
  actionLoading: boolean;
  onLoadDiff: (path: string, target?: string, staged?: boolean) => Promise<void>;
  onStageFiles: (paths: string[]) => Promise<void>;
  onUnstageFiles: (paths: string[]) => Promise<void>;
  onDiscardFiles: (paths: string[]) => Promise<void>;
}> = ({
  files,
  loading,
  loadingDiff,
  actionLoading,
  onLoadDiff,
  onStageFiles,
  onUnstageFiles,
  onDiscardFiles,
}) => {
  const discardableFiles = files.filter((file) => !file.conflicted);

  return (
    <div className="mb-3">
      <div className="mb-1 flex items-center justify-between gap-2 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
        <span>Working Tree</span>
        <div className="flex items-center gap-2">
          <span>{files.length} files</span>
          {discardableFiles.length > 1 && (
            <button
              type="button"
              onClick={() => { void onDiscardFiles(discardableFiles.map((file) => file.path)); }}
              disabled={actionLoading}
              className="h-7 shrink-0 rounded border border-rose-300 px-2 text-[11px] font-normal normal-case text-rose-700 hover:bg-rose-50 disabled:cursor-not-allowed disabled:opacity-50"
            >
              全部回滚
            </button>
          )}
        </div>
      </div>
      <div className="overflow-hidden rounded border border-border bg-background">
        {loading ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
        ) : files.length === 0 ? (
          <div className="px-3 py-2 text-xs text-muted-foreground">工作区干净</div>
        ) : files.map((file) => (
          <div
            key={`${file.path}:${file.staged ? 'staged' : 'worktree'}`}
            className="flex items-center gap-3 border-b border-border px-3 py-2 text-xs last:border-b-0"
          >
            <span
              className="shrink-0 rounded bg-muted px-2 py-0.5 text-[11px] text-muted-foreground"
              title={statusTitle[file.status]}
            >
              {statusLabel[file.status] || file.status}
            </span>
            <span className="min-w-0 flex-1 truncate font-mono text-[11px]" title={file.path}>
              {file.path}
            </span>
            {file.staged && (
              <button
                type="button"
                onClick={() => { void onLoadDiff(file.path, undefined, true); }}
                disabled={loadingDiff}
                className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                Staged Diff
              </button>
            )}
            {(file.unstaged || (!file.staged && file.status !== 'untracked')) && (
              <button
                type="button"
                onClick={() => { void onLoadDiff(file.path, undefined, false); }}
                disabled={loadingDiff}
                className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                Worktree Diff
              </button>
            )}
            {file.status === 'untracked' && (
              <button
                type="button"
                onClick={() => { void onLoadDiff(file.path, undefined, false); }}
                disabled={loadingDiff}
                className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                Preview Diff
              </button>
            )}
            {file.staged && (
              <button
                type="button"
                onClick={() => { void onUnstageFiles([file.path]); }}
                disabled={actionLoading || file.conflicted}
                className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                Unstage
              </button>
            )}
            {(!file.staged || file.unstaged || file.status === 'untracked') && (
              <button
                type="button"
                onClick={() => { void onStageFiles([file.path]); }}
                disabled={actionLoading || file.conflicted}
                className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                Stage
              </button>
            )}
            {!file.conflicted && (
              <button
                type="button"
                onClick={() => { void onDiscardFiles([file.path]); }}
                disabled={actionLoading}
                className="h-7 shrink-0 rounded border border-rose-300 px-2 text-[11px] text-rose-700 hover:bg-rose-50 disabled:cursor-not-allowed disabled:opacity-50"
              >
                回滚
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};
