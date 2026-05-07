import React from 'react';

import { cn } from '../../../lib/utils';
import type { GitFileDiff } from '../../../types';
import { diffLineView } from './gitBranchButtonShared';

export const DiffDialog: React.FC<{
  open: boolean;
  fileDiff: GitFileDiff | null;
  loading: boolean;
  error: string | null;
  onClose: () => void;
}> = ({
  open,
  fileDiff,
  loading,
  error,
  onClose,
}) => {
  if (!open) return null;

  const patch = fileDiff?.patch.trimEnd() || '';
  const lines = patch ? patch.split('\n') : [];
  const addedCount = lines.filter((line) => line.startsWith('+') && !line.startsWith('+++')).length;
  const deletedCount = lines.filter((line) => line.startsWith('-') && !line.startsWith('---')).length;
  const modeLabel = fileDiff?.target
    ? `对比 ${fileDiff.target}`
    : fileDiff?.staged
      ? 'Staged Diff'
      : 'Worktree Diff';

  return (
    <div className="fixed inset-0 z-[70] flex items-center justify-center bg-black/60 p-4">
      <div className="flex h-[88vh] min-h-0 w-full max-w-6xl flex-col overflow-hidden rounded-2xl border border-border bg-background shadow-2xl">
        <div className="border-b border-border bg-muted/30 px-4 py-3">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span className="rounded-full border border-border bg-background px-2 py-0.5">
                  {modeLabel}
                </span>
                <span className="rounded-full bg-emerald-500/15 px-2 py-0.5 text-emerald-600">
                  新增 {addedCount}
                </span>
                <span className="rounded-full bg-rose-500/15 px-2 py-0.5 text-rose-600">
                  删除 {deletedCount}
                </span>
                {loading && <span>加载中...</span>}
              </div>
              <div className="mt-2 truncate font-mono text-sm font-semibold text-foreground" title={fileDiff?.path || '文件 Diff'}>
                {fileDiff?.path || '文件 Diff'}
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <button
                type="button"
                disabled={!fileDiff?.patch}
                onClick={() => { if (fileDiff?.patch) void navigator.clipboard?.writeText(fileDiff.patch); }}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
              >
                复制 Diff
              </button>
              <button
                type="button"
                onClick={onClose}
                className="h-8 rounded border border-border px-3 text-xs hover:bg-accent"
              >
                关闭
              </button>
            </div>
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-hidden bg-background">
          {loading && !fileDiff ? (
            <div className="space-y-3 p-5">
              <div className="h-4 w-1/3 animate-pulse rounded bg-muted" />
              <div className="h-4 w-2/3 animate-pulse rounded bg-muted" />
              <div className="h-4 w-1/2 animate-pulse rounded bg-muted" />
            </div>
          ) : error && !fileDiff ? (
            <div className="m-4 rounded-lg border border-destructive/30 bg-destructive/10 p-4 text-sm text-destructive">
              {error}
            </div>
          ) : lines.length === 0 ? (
            <div className="p-5 text-sm text-muted-foreground">没有 diff 内容</div>
          ) : (
            <div className="h-full overflow-auto overscroll-contain">
              <div className="min-w-max py-3 font-mono text-[12px] leading-6">
                {lines.map((line, index) => {
                  const view = diffLineView(line);
                  return (
                    <div
                      key={`${index}:${line}`}
                      className={cn('grid grid-cols-[4rem_minmax(36rem,1fr)] border-l-4 pr-5', view.className)}
                    >
                      <span className="select-none border-r border-border/70 px-3 text-right text-muted-foreground">
                        {index + 1}
                      </span>
                      <span className="whitespace-pre px-3">{view.content}</span>
                    </div>
                  );
                })}
              </div>
            </div>
          )}
        </div>

        <div className="flex items-center justify-between gap-3 border-t border-border bg-background px-4 py-2 text-[11px] text-muted-foreground">
          <span>左侧色块标识类型：绿色新增，红色删除，黄色是变更位置。</span>
          {fileDiff?.target && <span className="truncate">Target: {fileDiff.target}</span>}
        </div>
      </div>
    </div>
  );
};
