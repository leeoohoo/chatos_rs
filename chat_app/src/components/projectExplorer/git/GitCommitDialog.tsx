import React from 'react';

import { useI18n } from '../../../i18n/I18nProvider';
import type { GitStatusFile } from '../../../types';
import { getGitStatusLabel } from './gitBranchButtonShared';

const CommitFileGroup: React.FC<{
  title: string;
  hint: string;
  files: GitStatusFile[];
  selectedPaths: Set<string>;
  onTogglePath: (path: string) => void;
  onSetPathsSelected: (paths: string[], selected: boolean) => void;
}> = ({
  title,
  hint,
  files,
  selectedPaths,
  onTogglePath,
  onSetPathsSelected,
}) => {
  const { t } = useI18n();
  const paths = files.map((file) => file.path);
  const selectedCount = paths.filter((path) => selectedPaths.has(path)).length;
  const allSelected = selectedCount === paths.length;
  return (
    <div className="overflow-hidden rounded border border-border">
      <div className="flex items-center justify-between gap-3 border-b border-border bg-muted/30 px-3 py-2">
        <div className="min-w-0">
          <div className="text-xs font-medium text-foreground">
            {title} <span className="text-muted-foreground">({selectedCount}/{paths.length})</span>
          </div>
          <div className="text-[11px] text-muted-foreground">{hint}</div>
        </div>
        <button
          type="button"
          onClick={() => onSetPathsSelected(paths, !allSelected)}
          className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent"
        >
          {allSelected ? t('git.commit.deselectGroup') : t('git.commit.selectGroup')}
        </button>
      </div>
      {files.map((file) => (
        <label
          key={`${title}:${file.path}`}
          className="flex cursor-pointer items-center gap-3 border-b border-border px-3 py-2 text-sm last:border-b-0 hover:bg-accent/60"
        >
          <input
            type="checkbox"
            checked={selectedPaths.has(file.path)}
            onChange={() => onTogglePath(file.path)}
          />
          <span className="min-w-0 flex-1 truncate font-mono text-xs">{file.path}</span>
          <span className="shrink-0 rounded bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
            {getGitStatusLabel(file.status, t)}
          </span>
        </label>
      ))}
    </div>
  );
};

export const CommitDialog: React.FC<{
  files: GitStatusFile[];
  message: string;
  selectedPaths: Set<string>;
  actionLoading: boolean;
  onMessageChange: (value: string) => void;
  onTogglePath: (path: string) => void;
  onSetPathsSelected: (paths: string[], selected: boolean) => void;
  onCancel: () => void;
  onSubmit: () => void;
  onSubmitStagedOnly: () => void;
}> = ({
  files,
  message,
  selectedPaths,
  actionLoading,
  onMessageChange,
  onTogglePath,
  onSetPathsSelected,
  onCancel,
  onSubmit,
  onSubmitStagedOnly,
}) => {
  const { t } = useI18n();
  const stagedOnlyFiles = files.filter((file) => file.staged && !file.unstaged);
  const mixedFiles = files.filter((file) => file.staged && file.unstaged);
  const unstagedFiles = files.filter((file) => !file.staged && file.unstaged && file.status !== 'untracked');
  const untrackedFiles = files.filter((file) => file.status === 'untracked');
  const hasStagedFiles = stagedOnlyFiles.length > 0 || mixedFiles.length > 0;
  const groups = [
    { title: 'Staged only', hint: t('git.commit.hint.stagedOnly'), files: stagedOnlyFiles },
    {
      title: 'Mixed',
      hint: t('git.commit.hint.mixed'),
      files: mixedFiles,
    },
    { title: 'Unstaged', hint: t('git.commit.hint.unstaged'), files: unstagedFiles },
    { title: 'Untracked', hint: t('git.commit.hint.untracked'), files: untrackedFiles },
  ].filter((group) => group.files.length > 0);

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/35 p-4">
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-background shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <div className="text-sm font-medium text-foreground">Git Commit</div>
          <div className="mt-1 text-xs text-muted-foreground">
            {t('git.commit.description')}
          </div>
        </div>
        <div className="flex-1 overflow-y-auto p-4">
          <textarea
            value={message}
            onChange={(event) => onMessageChange(event.target.value)}
            placeholder="Commit message"
            className="min-h-20 w-full resize-y rounded border border-border bg-background px-3 py-2 text-sm outline-none focus:border-primary"
          />
          <div className="mt-3 space-y-3">
            {files.length === 0 ? (
              <div className="rounded border border-border px-3 py-2 text-sm text-muted-foreground">
                {t('git.commit.noFiles')}
              </div>
            ) : groups.map((group) => (
              <CommitFileGroup
                key={group.title}
                title={group.title}
                hint={group.hint}
                files={group.files}
                selectedPaths={selectedPaths}
                onTogglePath={onTogglePath}
                onSetPathsSelected={onSetPathsSelected}
              />
            ))}
          </div>
        </div>
        <div className="flex items-center justify-between gap-3 border-t border-border px-4 py-3">
          <span className="text-xs text-muted-foreground">
            {t('git.commit.selectedFiles', { count: selectedPaths.size })}
          </span>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={onCancel}
              disabled={actionLoading}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50"
            >
              {t('common.cancel')}
            </button>
            <button
              type="button"
              onClick={onSubmitStagedOnly}
              disabled={actionLoading || !message.trim() || !hasStagedFiles}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50"
              title={!hasStagedFiles ? t('git.commit.noStagedFiles') : undefined}
            >
              Commit staged only
            </button>
            <button
              type="button"
              onClick={onSubmit}
              disabled={actionLoading || !message.trim() || selectedPaths.size === 0}
              className="h-8 rounded bg-primary px-3 text-xs text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {actionLoading ? t('git.commit.committing') : 'Commit'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
