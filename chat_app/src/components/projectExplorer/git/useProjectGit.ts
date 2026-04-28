import { useCallback, useMemo, useState } from 'react';

import type {
  GitBranchesResult,
  GitClientInfo,
  GitStatusResult,
  GitSummary,
} from '../../../types';
import type { GitActionResponse } from '../../../lib/api/client/types';
import {
  normalizeGitAction,
  normalizeGitBranches,
  normalizeGitClientInfo,
  normalizeGitStatus,
  normalizeGitSummary,
} from '../../../lib/domain/git';
import { useDialogService } from '../../ui/DialogProvider';
import {
  actionErrorMessage,
  actionOutputMessage,
} from './projectGitHelpers';
import type {
  UseProjectGitOptions,
  UseProjectGitResult,
} from './projectGitTypes';
import { useProjectGitActions } from './useProjectGitActions';
import { useProjectGitCompare } from './useProjectGitCompare';
import { useProjectGitLifecycle } from './useProjectGitLifecycle';

export const useProjectGit = ({
  client,
  projectRoot,
  onRepositoryChanged,
}: UseProjectGitOptions): UseProjectGitResult => {
  const { confirm } = useDialogService();
  const [clientInfo, setClientInfo] = useState<GitClientInfo | null>(null);
  const [summary, setSummary] = useState<GitSummary | null>(null);
  const [branches, setBranches] = useState<GitBranchesResult | null>(null);
  const [status, setStatus] = useState<GitStatusResult | null>(null);
  const [loadingSummary, setLoadingSummary] = useState(false);
  const [loadingClientInfo, setLoadingClientInfo] = useState(false);
  const [loadingBranches, setLoadingBranches] = useState(false);
  const [loadingStatus, setLoadingStatus] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);

  const clearMessages = useCallback(() => {
    setError(null);
    setActionMessage(null);
  }, []);

  const refreshClientInfo = useCallback(async () => {
    setLoadingClientInfo(true);
    try {
      setClientInfo(normalizeGitClientInfo(await client.getGitClientInfo()));
    } catch (err) {
      setClientInfo({
        available: false,
        source: 'unknown',
        path: 'git',
        version: null,
        error: err instanceof Error ? err.message : '加载 Git 客户端信息失败',
        bundledCandidates: [],
      });
    } finally {
      setLoadingClientInfo(false);
    }
  }, [client]);

  const refreshSummary = useCallback(async () => {
    if (!projectRoot) {
      setSummary(null);
      return;
    }
    setLoadingSummary(true);
    setError(null);
    try {
      const raw = await client.getGitSummary(projectRoot);
      setSummary(normalizeGitSummary(raw));
    } catch (err) {
      setSummary(null);
      setError(err instanceof Error ? err.message : '加载 Git 状态失败');
    } finally {
      setLoadingSummary(false);
    }
  }, [client, projectRoot]);

  const loadDetails = useCallback(async () => {
    if (!projectRoot) return;
    setError(null);
    setLoadingBranches(true);
    setLoadingStatus(true);
    try {
      const [branchesRaw, statusRaw] = await Promise.all([
        client.getGitBranches(projectRoot),
        client.getGitStatus(projectRoot),
      ]);
      setBranches(normalizeGitBranches(branchesRaw));
      setStatus(normalizeGitStatus(statusRaw));
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载 Git 详情失败');
    } finally {
      setLoadingBranches(false);
      setLoadingStatus(false);
    }
  }, [client, projectRoot]);

  const runAction = useCallback(async (
    action: () => Promise<GitActionResponse>,
    fallbackMessage: string,
    repositoryChanging = false,
  ): Promise<boolean> => {
    setActionLoading(true);
    setError(null);
    setActionMessage(null);
    try {
      const result = normalizeGitAction(await action());
      if (result.summary) {
        setSummary(result.summary);
      } else {
        await refreshSummary();
      }
      await loadDetails();
      if (repositoryChanging) {
        await onRepositoryChanged?.();
      }
      if (!result.success) {
        setError(actionErrorMessage(result, fallbackMessage));
        return false;
      }
      setActionMessage(actionOutputMessage(result, fallbackMessage));
      return true;
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Git 操作失败');
      return false;
    } finally {
      setActionLoading(false);
    }
  }, [loadDetails, onRepositoryChanged, refreshSummary]);

  const {
    compareResult,
    fileDiff,
    loadingCompare,
    loadingDiff,
    clearCompare,
    clearFileDiff,
    compareBranch,
    loadFileDiff,
  } = useProjectGitCompare({
    client,
    projectRoot,
    setError,
  });

  const {
    fetchRemote,
    pullCurrent,
    pushCurrent,
    checkoutBranch,
    mergeBranch,
    createBranch,
    stageFiles,
    unstageFiles,
    commitStaged,
    commitSelected,
  } = useProjectGitActions({
    client,
    projectRoot,
    summary,
    confirm,
    runAction,
    setError,
  });

  useProjectGitLifecycle({
    clearCompare,
    setBranches,
    setStatus,
    setActionMessage,
    refreshClientInfo,
    refreshSummary,
  });

  return useMemo(() => ({
    clientInfo,
    summary,
    branches,
    status,
    compareResult,
    fileDiff,
    loadingSummary,
    loadingClientInfo,
    loadingBranches,
    loadingStatus,
    loadingCompare,
    loadingDiff,
    actionLoading,
    error,
    actionMessage,
    refreshClientInfo,
    refreshSummary,
    loadDetails,
    fetchRemote,
    pullCurrent,
    pushCurrent,
    checkoutBranch,
    mergeBranch,
    compareBranch,
    loadFileDiff,
    clearCompare,
    clearFileDiff,
    createBranch,
    stageFiles,
    unstageFiles,
    commitStaged,
    commitSelected,
    clearMessages,
  }), [
    actionLoading,
    actionMessage,
    branches,
    checkoutBranch,
    clearCompare,
    clearFileDiff,
    clearMessages,
    clientInfo,
    commitStaged,
    commitSelected,
    compareBranch,
    compareResult,
    createBranch,
    error,
    fetchRemote,
    fileDiff,
    loadDetails,
    loadFileDiff,
    mergeBranch,
    loadingBranches,
    loadingClientInfo,
    loadingCompare,
    loadingDiff,
    loadingStatus,
    loadingSummary,
    pullCurrent,
    pushCurrent,
    refreshClientInfo,
    refreshSummary,
    stageFiles,
    status,
    summary,
    unstageFiles,
  ]);
};
