// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type { FC } from 'react';
import { FileDiff } from 'lucide-react';

import { useI18n } from '../../i18n/I18nProvider';
import type {
  MessageTaskRunnerFileChange,
  MessageTaskRunnerRunOutputChangesResponse,
  MessageTaskRunnerRunOutputDiffResponse,
  MessageTaskRunnerTask,
} from '../../lib/api/client/types';
import { cn } from '../../lib/utils';
import { diffLineView } from '../projectExplorer/git/gitBranchButtonShared';
import { ModalShell } from './parts';

interface MessageTaskChangesModalProps {
  task: MessageTaskRunnerTask | null;
  changes: MessageTaskRunnerRunOutputChangesResponse | null;
  diff: MessageTaskRunnerRunOutputDiffResponse | null;
  selectedPath: string | null;
  loadingChanges: boolean;
  loadingDiff: boolean;
  error: string | null;
  onSelectFile: (file: MessageTaskRunnerFileChange) => void | Promise<void>;
  onClose: () => void;
}

const statusLabel = (status: string): string => {
  switch (status) {
    case 'added':
      return '新增';
    case 'modified':
      return '修改';
    case 'deleted':
      return '删除';
    default:
      return status || '-';
  }
};

const statusTone = (status: string): string => {
  switch (status) {
    case 'added':
      return 'bg-emerald-500/12 text-emerald-700 dark:text-emerald-200';
    case 'modified':
      return 'bg-sky-500/12 text-sky-700 dark:text-sky-200';
    case 'deleted':
      return 'bg-rose-500/12 text-rose-700 dark:text-rose-200';
    default:
      return 'bg-muted text-muted-foreground';
  }
};

const countValue = (
  changes: MessageTaskRunnerRunOutputChangesResponse | null,
  key: 'added' | 'modified' | 'deleted',
): number => Number(changes?.counts?.[key] || 0);

export const MessageTaskChangesModal: FC<MessageTaskChangesModalProps> = ({
  task,
  changes,
  diff,
  selectedPath,
  loadingChanges,
  loadingDiff,
  error,
  onSelectFile,
  onClose,
}) => {
  const { t } = useI18n();
  if (!task) {
    return null;
  }

  const files = Array.isArray(changes?.files) ? changes.files : [];
  const patch = diff?.patch?.trimEnd() || '';
  const lines = patch ? patch.split('\n') : [];

  return (
    <ModalShell
      title="文件变更"
      subtitle={task.title || task.id}
      onClose={onClose}
      widthClassName="max-w-6xl"
    >
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="rounded-full bg-emerald-500/12 px-2 py-1 text-emerald-700 dark:text-emerald-200">
          新增 {countValue(changes, 'added')}
        </span>
        <span className="rounded-full bg-sky-500/12 px-2 py-1 text-sky-700 dark:text-sky-200">
          修改 {countValue(changes, 'modified')}
        </span>
        <span className="rounded-full bg-rose-500/12 px-2 py-1 text-rose-700 dark:text-rose-200">
          删除 {countValue(changes, 'deleted')}
        </span>
        {loadingChanges ? (
          <span className="text-muted-foreground">正在加载...</span>
        ) : null}
      </div>

      {error ? (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          {error}
        </div>
      ) : null}

      <div className="grid min-h-[34rem] overflow-hidden rounded-lg border border-border bg-background md:grid-cols-[18rem_minmax(0,1fr)]">
        <div className="min-h-0 border-b border-border md:border-b-0 md:border-r">
          {loadingChanges && files.length === 0 ? (
            <div className="space-y-2 p-3">
              <div className="h-8 animate-pulse rounded bg-muted" />
              <div className="h-8 animate-pulse rounded bg-muted" />
              <div className="h-8 animate-pulse rounded bg-muted" />
            </div>
          ) : files.length === 0 ? (
            <div className="p-4 text-sm text-muted-foreground">本次运行没有记录到文件变更。</div>
          ) : (
            <div className="max-h-[34rem] overflow-auto">
              {files.map((file) => {
                const active = selectedPath === file.path;
                return (
                  <button
                    key={`${file.status}:${file.path}`}
                    type="button"
                    className={cn(
                      'flex w-full items-start gap-2 border-b border-border px-3 py-2 text-left text-xs hover:bg-accent',
                      active && 'bg-accent',
                    )}
                    onClick={() => { void onSelectFile(file); }}
                  >
                    <span className={cn('shrink-0 rounded px-1.5 py-0.5 text-[11px]', statusTone(file.status))}>
                      {statusLabel(file.status)}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-mono text-[11px] text-foreground" title={file.path}>
                        {file.path}
                      </span>
                      <span className="mt-1 block text-[11px] text-muted-foreground">
                        +{file.added_lines || 0} / -{file.deleted_lines || 0}
                        {file.binary ? ' · 二进制' : ''}
                      </span>
                    </span>
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div className="min-h-0 bg-background">
          {loadingDiff ? (
            <div className="space-y-3 p-5">
              <div className="h-4 w-1/3 animate-pulse rounded bg-muted" />
              <div className="h-4 w-2/3 animate-pulse rounded bg-muted" />
              <div className="h-4 w-1/2 animate-pulse rounded bg-muted" />
            </div>
          ) : !selectedPath ? (
            <div className="flex h-full min-h-[34rem] items-center justify-center p-8 text-sm text-muted-foreground">
              选择左侧文件查看 diff。
            </div>
          ) : !patch ? (
            <div className="flex h-full min-h-[34rem] items-center justify-center p-8 text-center text-sm text-muted-foreground">
              <div className="space-y-2">
                <FileDiff className="mx-auto h-8 w-8 opacity-60" />
                <p>{diff?.message || '该文件没有可展示的 diff。'}</p>
              </div>
            </div>
          ) : (
            <div className="h-[34rem] overflow-auto overscroll-contain">
              <div className="min-w-max py-3 font-mono text-[12px] leading-6">
                {lines.map((line, index) => {
                  const view = diffLineView(line, t);
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
      </div>
    </ModalShell>
  );
};
