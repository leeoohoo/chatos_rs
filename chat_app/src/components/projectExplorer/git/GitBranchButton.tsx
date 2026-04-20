import React, { useMemo, useRef, useState } from 'react';

import type {
  GitBranchInfo,
  GitClientInfo,
  GitCompareResult,
  GitFileDiff,
  GitStatusFile,
} from '../../../types';
import { cn } from '../../../lib/utils';
import { type ProjectGitApiClient, useProjectGit } from './useProjectGit';

interface GitBranchButtonProps {
  client: ProjectGitApiClient;
  projectRoot: string;
  onRepositoryChanged?: () => Promise<void> | void;
}

const statusLabel: Record<string, string> = {
  added: '新增',
  modified: '修改',
  deleted: '删除',
  renamed: '重命名',
  copied: '复制',
  untracked: '未跟踪',
  conflicted: '冲突',
};

const statusTitle: Record<string, string> = {
  untracked: 'Git 还没有纳入版本管理的新文件，Stage 后才会进入本次提交。',
  conflicted: '文件存在合并冲突，需要解决后再提交。',
};

export const GitBranchButton: React.FC<GitBranchButtonProps> = ({
  client,
  projectRoot,
  onRepositoryChanged,
}) => {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [newBranchName, setNewBranchName] = useState('');
  const [commitOpen, setCommitOpen] = useState(false);
  const [diffDialogOpen, setDiffDialogOpen] = useState(false);
  const [commitMessage, setCommitMessage] = useState('');
  const [selectedCommitPaths, setSelectedCommitPaths] = useState<Set<string>>(new Set());
  const panelRef = useRef<HTMLDivElement | null>(null);
  const git = useProjectGit({ client, projectRoot, onRepositoryChanged });

  const branchLabel = useMemo(() => {
    if (git.loadingSummary && !git.summary) return 'Git 检查中...';
    if (!git.summary?.isRepo) return '无 Git 仓库';
    if (git.summary.detached) return `detached: ${git.summary.head || '-'}`;
    return git.summary.currentBranch || '未知分支';
  }, [git.loadingSummary, git.summary]);

  const changeCount = git.summary
    ? git.summary.changes.staged
      + git.summary.changes.unstaged
      + git.summary.changes.untracked
      + git.summary.changes.conflicted
    : 0;

  const filteredBranches = useMemo(() => {
    const keyword = query.trim().toLowerCase();
    const locals = git.branches?.locals || [];
    const remotes = git.branches?.remotes || [];
    if (!keyword) return { locals, remotes };
    const matches = (branch: GitBranchInfo) => [
      branch.name,
      branch.shortName,
      branch.upstream,
      branch.lastCommitSubject,
    ].some((value) => (value || '').toLowerCase().includes(keyword));
    return {
      locals: locals.filter(matches),
      remotes: remotes.filter(matches),
    };
  }, [git.branches, query]);

  const allStatusFiles = git.status?.files || [];
  const selectableCommitFiles = allStatusFiles.filter((file) => !file.conflicted);

  const toggleOpen = () => {
    setOpen((value) => {
      const next = !value;
      if (next) {
        git.clearMessages();
        void git.loadDetails();
      }
      return next;
    });
  };

  const toggleCommitPath = (path: string) => {
    setSelectedCommitPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  };

  const setCommitPathsSelected = (paths: string[], selected: boolean) => {
    setSelectedCommitPaths((prev) => {
      const next = new Set(prev);
      paths.forEach((path) => {
        if (selected) {
          next.add(path);
        } else {
          next.delete(path);
        }
      });
      return next;
    });
  };

  const openCommitDialog = () => {
    setCommitOpen(true);
    setSelectedCommitPaths(new Set(selectableCommitFiles.map((file) => file.path)));
  };

  const openDiffDialog = async (path: string, target?: string, staged?: boolean) => {
    setDiffDialogOpen(true);
    await git.loadFileDiff(path, target, staged);
  };

  const closeDiffDialog = () => {
    setDiffDialogOpen(false);
    git.clearFileDiff();
  };

  const submitCommit = async () => {
    const success = await git.commitSelected(commitMessage, Array.from(selectedCommitPaths));
    if (!success) return;
    setCommitMessage('');
    setCommitOpen(false);
    setSelectedCommitPaths(new Set());
  };

  const submitStagedCommit = async () => {
    const success = await git.commitStaged(commitMessage);
    if (!success) return;
    setCommitMessage('');
    setCommitOpen(false);
    setSelectedCommitPaths(new Set());
  };

  return (
    <div className="relative" ref={panelRef}>
      <button
        type="button"
        onClick={toggleOpen}
        className={cn(
          'inline-flex h-8 max-w-56 items-center gap-2 rounded-md border border-border bg-background px-3 text-xs text-foreground shadow-sm transition-colors hover:bg-accent',
          git.summary?.dirty && 'border-amber-400/70'
        )}
        title={branchLabel}
      >
        <span className="text-muted-foreground">Git</span>
        <span className="truncate font-medium">{branchLabel}</span>
        {git.summary?.isRepo && git.summary.ahead > 0 && (
          <span className="text-[11px] text-emerald-600">↑{git.summary.ahead}</span>
        )}
        {git.summary?.isRepo && git.summary.behind > 0 && (
          <span className="text-[11px] text-sky-600">↓{git.summary.behind}</span>
        )}
        {changeCount > 0 && (
          <span className="rounded bg-amber-500/15 px-1.5 py-0.5 text-[11px] text-amber-700">
            +{changeCount}
          </span>
        )}
        <span className="text-muted-foreground">{open ? '⌃' : '⌄'}</span>
      </button>

      {open && (
        <div className="absolute right-0 top-10 z-50 flex max-h-[78vh] w-[min(720px,calc(100vw-2rem))] flex-col overflow-hidden rounded-lg border border-border bg-popover text-popover-foreground shadow-xl">
          <div className="border-b border-border p-3">
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="搜索分支和动作"
              className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus:border-primary"
              autoFocus
            />
          </div>

          <div className="flex-1 overflow-y-auto p-2">
            {git.error && (
              <div className="mb-2 rounded border border-destructive/30 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                {git.error}
              </div>
            )}
            {git.actionMessage && (
              <div className="mb-2 rounded border border-emerald-500/30 bg-emerald-500/5 px-3 py-2 text-xs text-emerald-700">
                {git.actionMessage}
              </div>
            )}
            <GitClientInfoBlock
              clientInfo={git.clientInfo}
              loading={git.loadingClientInfo}
              onRefresh={git.refreshClientInfo}
            />

            {!git.summary?.isRepo ? (
              <div className="space-y-3 p-3 text-sm text-muted-foreground">
                <div>当前项目目录不是 Git 仓库。</div>
                <button
                  type="button"
                  onClick={() => { void git.refreshSummary(); }}
                  className="h-8 rounded border border-border px-3 text-xs hover:bg-accent"
                >
                  刷新
                </button>
              </div>
            ) : (
              <>
                <GitSummaryBlock
                  branchLabel={branchLabel}
                  changeCount={changeCount}
                  summary={git.summary}
                />
                <GitActionRows
                  actionLoading={git.actionLoading}
                  onFetch={git.fetchRemote}
                  onPull={git.pullCurrent}
                  onPush={git.pushCurrent}
                  onOpenCommit={openCommitDialog}
                />
                <StatusSection
                  files={allStatusFiles}
                  loading={git.loadingStatus}
                  loadingDiff={git.loadingDiff}
                  actionLoading={git.actionLoading}
                  onLoadDiff={openDiffDialog}
                  onStageFiles={git.stageFiles}
                  onUnstageFiles={git.unstageFiles}
                />
                <ComparePanel
                  compareResult={git.compareResult}
                  loadingCompare={git.loadingCompare}
                  loadingDiff={git.loadingDiff}
                  onLoadFileDiff={openDiffDialog}
                  onClear={git.clearCompare}
                />
                <NewBranchRow
                  value={newBranchName}
                  disabled={git.actionLoading}
                  onChange={setNewBranchName}
                  onCreate={async () => {
                    await git.createBranch(newBranchName, git.summary?.currentBranch || undefined);
                    setNewBranchName('');
                  }}
                />
                <BranchSection
                  title="Local"
                  branches={filteredBranches.locals}
                  loading={git.loadingBranches}
                  actionLoading={git.actionLoading}
                  loadingCompare={git.loadingCompare}
                  operationState={git.summary?.operationState}
                  onCheckout={git.checkoutBranch}
                  onMerge={git.mergeBranch}
                  onCompare={git.compareBranch}
                />
                <BranchSection
                  title="Remote"
                  branches={filteredBranches.remotes}
                  loading={git.loadingBranches}
                  actionLoading={git.actionLoading}
                  loadingCompare={git.loadingCompare}
                  operationState={git.summary?.operationState}
                  onCheckout={git.checkoutBranch}
                  onMerge={git.mergeBranch}
                  onCompare={git.compareBranch}
                />
              </>
            )}
          </div>

          <div className="flex items-center justify-between border-t border-border px-3 py-2">
            <span className="text-[11px] text-muted-foreground">
              {git.loadingSummary || git.loadingBranches || git.loadingStatus ? '加载中...' : projectRoot}
            </span>
            <button
              type="button"
              onClick={() => setOpen(false)}
              className="h-7 rounded border border-border px-2 text-xs hover:bg-accent"
            >
              关闭
            </button>
          </div>
        </div>
      )}

      {commitOpen && (
        <CommitDialog
          files={selectableCommitFiles}
          message={commitMessage}
          selectedPaths={selectedCommitPaths}
          actionLoading={git.actionLoading}
          onMessageChange={setCommitMessage}
          onTogglePath={toggleCommitPath}
          onSetPathsSelected={setCommitPathsSelected}
          onCancel={() => setCommitOpen(false)}
          onSubmit={() => { void submitCommit(); }}
          onSubmitStagedOnly={() => { void submitStagedCommit(); }}
        />
      )}

      <DiffDialog
        open={diffDialogOpen}
        fileDiff={git.fileDiff}
        loading={git.loadingDiff}
        error={git.error}
        onClose={closeDiffDialog}
      />
    </div>
  );
};

const GitSummaryBlock: React.FC<{
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

const GitActionRows: React.FC<{
  actionLoading: boolean;
  onFetch: () => Promise<void>;
  onPull: () => Promise<void>;
  onPush: () => Promise<void>;
  onOpenCommit: () => void;
}> = ({ actionLoading, onFetch, onPull, onPush, onOpenCommit }) => {
  const actions = [
    { label: 'Fetch', run: onFetch },
    { label: 'Pull --ff-only', run: onPull },
    { label: 'Push', run: onPush },
  ];
  return (
    <div className="mb-2 grid grid-cols-2 gap-2">
      {actions.map((action) => (
        <button
          key={action.label}
          type="button"
          onClick={() => { void action.run(); }}
          disabled={actionLoading}
          className="h-8 rounded border border-border px-3 text-left text-xs hover:bg-accent disabled:opacity-50"
        >
          {action.label}
        </button>
      ))}
      <button
        type="button"
        onClick={onOpenCommit}
        disabled={actionLoading}
        className="h-8 rounded border border-border px-3 text-left text-xs hover:bg-accent disabled:opacity-50"
      >
        Commit...
      </button>
    </div>
  );
};

const gitClientSourceLabel: Record<string, string> = {
  env: '环境变量 Git',
  bundled: '内置 Git',
  system: '系统 Git',
  unknown: '未知 Git',
};

const GitClientInfoBlock: React.FC<{
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
        : 'border-border bg-background text-muted-foreground'
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

const NewBranchRow: React.FC<{
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
  onCreate: () => Promise<void>;
}> = ({ value, disabled, onChange, onCreate }) => (
  <div className="mb-3 flex gap-2">
    <input
      value={value}
      onChange={(event) => onChange(event.target.value)}
      placeholder="New Branch..."
      className="h-8 min-w-0 flex-1 rounded border border-border bg-background px-2 text-xs outline-none focus:border-primary"
    />
    <button
      type="button"
      disabled={disabled || !value.trim()}
      onClick={() => { void onCreate(); }}
      className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50"
    >
      创建
    </button>
  </div>
);

const StatusSection: React.FC<{
  files: GitStatusFile[];
  loading: boolean;
  loadingDiff: boolean;
  actionLoading: boolean;
  onLoadDiff: (path: string, target?: string, staged?: boolean) => Promise<void>;
  onStageFiles: (paths: string[]) => Promise<void>;
  onUnstageFiles: (paths: string[]) => Promise<void>;
}> = ({
  files,
  loading,
  loadingDiff,
  actionLoading,
  onLoadDiff,
  onStageFiles,
  onUnstageFiles,
}) => (
  <div className="mb-3">
    <div className="mb-1 flex items-center justify-between px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
      <span>Working Tree</span>
      <span>{files.length} files</span>
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
        </div>
      ))}
    </div>
  </div>
);

const ComparePanel: React.FC<{
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
  if (loadingCompare && !compareResult) {
    return (
      <div className="mb-3 flex items-center justify-between gap-3 rounded border border-border bg-background px-3 py-2 text-xs text-muted-foreground">
        <span>正在对比分支...</span>
        <button
          type="button"
          onClick={onClear}
          className="h-7 shrink-0 rounded border border-border px-2 text-[11px] hover:bg-accent"
        >
          返回分支列表
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
            返回分支列表
          </button>
          <div className="min-w-0">
            <div className="truncate font-medium text-foreground">
              {compareResult.current} ↔ {compareResult.target}
            </div>
            <div className="text-[11px] text-muted-foreground">
              {compareResult.files.length} 个文件，{targetCommits.length} 个目标分支提交，{currentCommits.length} 个当前分支提交
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={onClear}
          className="h-7 shrink-0 rounded border border-border px-2 text-[11px] text-muted-foreground hover:bg-accent hover:text-foreground"
        >
          清除
        </button>
      </div>

      {compareResult && (
        <div className="grid gap-2 p-2 md:grid-cols-[minmax(0,1fr)_220px]">
          <div className="overflow-hidden rounded border border-border">
            {compareResult.files.length === 0 ? (
              <div className="px-3 py-2 text-xs text-muted-foreground">没有文件差异</div>
            ) : compareResult.files.map((file) => (
              <button
                key={`${file.status}:${file.oldPath || ''}:${file.path}`}
                type="button"
                onClick={() => { void onLoadFileDiff(file.path, compareResult.target); }}
                disabled={loadingDiff}
                className="flex w-full items-center gap-2 border-b border-border px-3 py-2 text-left text-xs last:border-b-0 hover:bg-accent disabled:opacity-50"
              >
                <span className="shrink-0 rounded bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
                  {statusLabel[file.status] || file.status}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px]" title={file.path}>
                  {file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
                </span>
              </button>
            ))}
          </div>

          <div className="space-y-2">
            <CommitList title="Target Only" commits={targetCommits} />
            <CommitList title="Current Only" commits={currentCommits} />
          </div>
        </div>
      )}
    </div>
  );
};

interface DiffLineView {
  content: string;
  className: string;
}

const formatDiffFilePath = (value: string): string => (
  value.replace(/^[ab]\//, '').trim()
);

const formatDiffHeader = (line: string): string => {
  const match = line.match(/^diff --git\s+a\/(.+?)\s+b\/(.+)$/);
  if (!match) return line;
  const oldPath = formatDiffFilePath(match[1]);
  const newPath = formatDiffFilePath(match[2]);
  return oldPath === newPath ? oldPath : `${oldPath} -> ${newPath}`;
};

const formatDiffHunk = (line: string): string => {
  const match = line.match(/^@@\s+-(\S+)\s+\+(\S+)\s+@@\s*(.*)$/);
  if (!match) return line;
  const oldRange = match[1].replace(/^\+|-/, '');
  const newRange = match[2].replace(/^\+|-/, '');
  const suffix = match[3] ? ` · ${match[3]}` : '';
  return `旧 ${oldRange} / 新 ${newRange}${suffix}`;
};

const diffLineView = (line: string): DiffLineView => {
  if (line.startsWith('diff --git')) {
    return {
      content: formatDiffHeader(line),
      className: 'border-l-sky-500 bg-sky-50 text-sky-950 dark:bg-sky-950/35 dark:text-sky-100',
    };
  }
  if (line.startsWith('@@')) {
    return {
      content: formatDiffHunk(line),
      className: 'border-l-amber-500 bg-amber-50 text-amber-950 dark:bg-amber-950/35 dark:text-amber-100',
    };
  }
  if (line.startsWith('+++')) {
    return {
      content: formatDiffFilePath(line.replace(/^\+\+\+\s*/, '')),
      className: 'border-l-muted-foreground/50 bg-muted/60 text-muted-foreground',
    };
  }
  if (line.startsWith('---')) {
    return {
      content: formatDiffFilePath(line.replace(/^---\s*/, '')),
      className: 'border-l-muted-foreground/50 bg-muted/60 text-muted-foreground',
    };
  }
  if (line.startsWith('+')) {
    return {
      content: line.slice(1) || ' ',
      className: 'border-l-emerald-500 bg-emerald-100/80 text-emerald-950 dark:bg-emerald-950/45 dark:text-emerald-50',
    };
  }
  if (line.startsWith('-')) {
    return {
      content: line.slice(1) || ' ',
      className: 'border-l-rose-500 bg-rose-100/80 text-rose-950 dark:bg-rose-950/45 dark:text-rose-50',
    };
  }
  return {
    content: line.startsWith(' ') ? line.slice(1) : line || ' ',
    className: 'border-l-transparent text-foreground',
  };
};

const DiffDialog: React.FC<{
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

const CommitList: React.FC<{
  title: string;
  commits: GitCompareResult['commits'];
}> = ({ title, commits }) => (
  <div className="overflow-hidden rounded border border-border">
    <div className="border-b border-border px-2 py-1 text-[11px] font-medium text-muted-foreground">
      {title}
    </div>
    {commits.length === 0 ? (
      <div className="px-2 py-2 text-[11px] text-muted-foreground">无</div>
    ) : commits.slice(0, 6).map((commit) => (
      <div key={`${title}:${commit.hash}`} className="border-b border-border px-2 py-1.5 text-[11px] last:border-b-0">
        <span className="font-mono text-muted-foreground">{commit.hash}</span>
        <span className="ml-2 text-foreground">{commit.subject || '(no subject)'}</span>
      </div>
    ))}
    {commits.length > 6 && (
      <div className="px-2 py-1.5 text-[11px] text-muted-foreground">
        还有 {commits.length - 6} 个提交
      </div>
    )}
  </div>
);

const BranchSection: React.FC<{
  title: string;
  branches: GitBranchInfo[];
  loading: boolean;
  actionLoading: boolean;
  loadingCompare: boolean;
  operationState?: string | null;
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
  onCheckout,
  onMerge,
  onCompare,
}) => (
  <div className="mb-3">
    <div className="mb-1 px-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
      {title}
    </div>
    <div className="overflow-hidden rounded border border-border">
      {loading ? (
        <div className="px-3 py-2 text-xs text-muted-foreground">加载中...</div>
      ) : branches.length === 0 ? (
        <div className="px-3 py-2 text-xs text-muted-foreground">没有匹配的分支</div>
      ) : branches.map((branch) => (
        <div
          key={`${title}:${branch.name}`}
          className={cn(
            'flex items-center justify-between gap-3 border-b border-border px-3 py-2 text-xs last:border-b-0',
            branch.current && 'bg-accent/70'
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
              title={operationState ? `当前处于 ${operationState} 状态，不能 Merge` : undefined}
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
        </div>
      ))}
    </div>
  </div>
);

const CommitDialog: React.FC<{
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
  const stagedOnlyFiles = files.filter((file) => file.staged && !file.unstaged);
  const mixedFiles = files.filter((file) => file.staged && file.unstaged);
  const unstagedFiles = files.filter((file) => !file.staged && file.unstaged && file.status !== 'untracked');
  const untrackedFiles = files.filter((file) => file.status === 'untracked');
  const hasStagedFiles = stagedOnlyFiles.length > 0 || mixedFiles.length > 0;
  const groups = [
    { title: 'Staged only', hint: '只包含已 staged 内容', files: stagedOnlyFiles },
    {
      title: 'Mixed',
      hint: '同一路径同时有 staged 和 unstaged；普通 Commit 会先 Stage 整个文件',
      files: mixedFiles,
    },
    { title: 'Unstaged', hint: '提交前会先 Stage 选中的文件', files: unstagedFiles },
    { title: 'Untracked', hint: '提交前会先 git add', files: untrackedFiles },
  ].filter((group) => group.files.length > 0);

  return (
    <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/35 p-4">
      <div className="flex max-h-[80vh] w-full max-w-2xl flex-col overflow-hidden rounded-lg border border-border bg-background shadow-xl">
        <div className="border-b border-border px-4 py-3">
          <div className="text-sm font-medium text-foreground">Git Commit</div>
          <div className="mt-1 text-xs text-muted-foreground">
            普通 Commit 会先 Stage 选中的文件；如果只想提交 index 里已有内容，请用 Commit staged only。
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
                没有可提交文件
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
            已选择 {selectedPaths.size} 个文件
          </span>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={onCancel}
              disabled={actionLoading}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50"
            >
              取消
            </button>
            <button
              type="button"
              onClick={onSubmitStagedOnly}
              disabled={actionLoading || !message.trim() || !hasStagedFiles}
              className="h-8 rounded border border-border px-3 text-xs hover:bg-accent disabled:opacity-50"
              title={!hasStagedFiles ? '没有 staged 文件' : undefined}
            >
              Commit staged only
            </button>
            <button
              type="button"
              onClick={onSubmit}
              disabled={actionLoading || !message.trim() || selectedPaths.size === 0}
              className="h-8 rounded bg-primary px-3 text-xs text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
            >
              {actionLoading ? '提交中...' : 'Commit'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};

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
          {allSelected ? '取消本组' : '选择本组'}
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
            {statusLabel[file.status] || file.status}
          </span>
        </label>
      ))}
    </div>
  );
};

export default GitBranchButton;
