import { useMemo, useRef, useState } from 'react';

import { useProjectChangeSummaryRealtime } from '../../../lib/realtime/useProjectChangeSummaryRealtime';
import type { GitBranchInfo } from '../../../types';
import type { ProjectGitApiClient } from './projectGitTypes';
import { useProjectGit } from './useProjectGit';

interface UseGitBranchButtonModelOptions {
  client: ProjectGitApiClient;
  projectId?: string | null;
  projectRoot: string;
  onRepositoryChanged?: () => Promise<void> | void;
  onRepositorySelectionChange?: (repoRoot: string | null) => Promise<void> | void;
}

export const useGitBranchButtonModel = ({
  client,
  projectId,
  projectRoot,
  onRepositoryChanged,
  onRepositorySelectionChange,
}: UseGitBranchButtonModelOptions) => {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [newBranchName, setNewBranchName] = useState('');
  const [commitOpen, setCommitOpen] = useState(false);
  const [diffDialogOpen, setDiffDialogOpen] = useState(false);
  const [commitMessage, setCommitMessage] = useState('');
  const [selectedCommitPaths, setSelectedCommitPaths] = useState<Set<string>>(new Set());
  const panelRef = useRef<HTMLDivElement | null>(null);
  const git = useProjectGit({
    client,
    projectRoot,
    open,
    onRepositoryChanged,
    onRepositorySelectionChange,
  });

  useProjectChangeSummaryRealtime({
    projectId,
    enabled: Boolean(projectId),
    onInvalidate: async () => {
      git.markSummaryStale();
      git.markDetailsStale();
      await git.refreshSummary();
      if (open) {
        await git.loadDetails();
      }
    },
  });

  const branchLabel = useMemo(() => {
    if (git.loadingSummary && !git.summary) return 'Git 检查中...';
    if (!git.summary?.isRepo && git.availableRepositories.length > 0) {
      return `发现 ${git.availableRepositories.length} 个仓库`;
    }
    if (!git.summary?.isRepo) return '无 Git 仓库';
    if (git.summary.detached) return `detached: ${git.summary.head || '-'}`;
    return git.summary.currentBranch || '未知分支';
  }, [git.availableRepositories.length, git.loadingSummary, git.summary]);

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

  const closePanel = () => {
    setOpen(false);
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

  const closeCommitDialog = () => {
    setCommitOpen(false);
  };

  const openDiffDialog = async (path: string, target?: string, staged?: boolean) => {
    setDiffDialogOpen(true);
    await git.loadFileDiff(path, target, staged);
  };

  const closeDiffDialog = () => {
    setDiffDialogOpen(false);
    git.clearFileDiff();
  };

  const createBranch = async () => {
    await git.createBranch(newBranchName, git.summary?.currentBranch || undefined);
    setNewBranchName('');
  };

  const resetCommitDialog = () => {
    setCommitMessage('');
    setCommitOpen(false);
    setSelectedCommitPaths(new Set());
  };

  const submitCommit = async () => {
    const success = await git.commitSelected(commitMessage, Array.from(selectedCommitPaths));
    if (!success) return;
    resetCommitDialog();
  };

  const submitStagedCommit = async () => {
    const success = await git.commitStaged(commitMessage);
    if (!success) return;
    resetCommitDialog();
  };

  return {
    allStatusFiles,
    branchLabel,
    changeCount,
    closeCommitDialog,
    closeDiffDialog,
    closePanel,
    commitMessage,
    commitOpen,
    createBranch,
    diffDialogOpen,
    filteredBranches,
    git,
    newBranchName,
    open,
    openCommitDialog,
    openDiffDialog,
    panelRef,
    projectRoot,
    query,
    gitAvailableRepositories: git.availableRepositories,
    activeRepoRoot: git.activeRepoRoot,
    selectableCommitFiles,
    selectedCommitPaths,
    setCommitMessage,
    setCommitPathsSelected,
    setNewBranchName,
    setQuery,
    submitCommit,
    submitStagedCommit,
    selectRepository: git.selectRepository,
    toggleCommitPath,
    toggleOpen,
  };
};

export type GitBranchButtonModel = ReturnType<typeof useGitBranchButtonModel>;
