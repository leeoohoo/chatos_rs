import { useCallback, useMemo, useRef, useState } from 'react';

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
  getGitClientInfoInflight,
  getGitDetailsInflight,
  getGitSummaryInflight,
  markGitDetailsCacheStale,
  markGitSummaryCacheStale,
  peekGitClientInfoCacheEntry,
  peekGitDetailsCacheEntry,
  peekGitSummaryCacheEntry,
  setGitClientInfoCacheEntry,
  setGitClientInfoInflight,
  setGitDetailsCacheEntry,
  setGitDetailsInflight,
  setGitSummaryCacheEntry,
  setGitSummaryInflight,
} from './cache';
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
  open = false,
  onRepositoryChanged,
}: UseProjectGitOptions): UseProjectGitResult => {
  const { confirm } = useDialogService();
  const [clientInfo, setClientInfo] = useState<GitClientInfo | null>(
    () => peekGitClientInfoCacheEntry(client)?.clientInfo || null,
  );
  const [summary, setSummary] = useState<GitSummary | null>(
    () => peekGitSummaryCacheEntry(client, projectRoot)?.summary || null,
  );
  const [branches, setBranches] = useState<GitBranchesResult | null>(
    () => peekGitDetailsCacheEntry(client, projectRoot)?.branches || null,
  );
  const [status, setStatus] = useState<GitStatusResult | null>(
    () => peekGitDetailsCacheEntry(client, projectRoot)?.status || null,
  );
  const [loadingSummary, setLoadingSummary] = useState(false);
  const [loadingClientInfo, setLoadingClientInfo] = useState(false);
  const [loadingBranches, setLoadingBranches] = useState(false);
  const [loadingStatus, setLoadingStatus] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const summaryRef = useRef<GitSummary | null>(null);
  const branchesRef = useRef<GitBranchesResult | null>(null);
  const statusRef = useRef<GitStatusResult | null>(null);
  const summaryLoadedRootRef = useRef<string | null>(null);
  const detailsLoadedRootRef = useRef<string | null>(null);
  const summaryStaleRef = useRef(true);
  const detailsStaleRef = useRef(true);

  const clearMessages = useCallback(() => {
    setError(null);
    setActionMessage(null);
  }, []);

  const hydrateCachedState = useCallback((nextProjectRoot: string) => {
    setClientInfo(peekGitClientInfoCacheEntry(client)?.clientInfo || null);
    const cachedSummary = peekGitSummaryCacheEntry(client, nextProjectRoot);
    setSummary(cachedSummary?.summary || null);
    summaryRef.current = cachedSummary?.summary || null;
    summaryLoadedRootRef.current = cachedSummary ? nextProjectRoot : null;
    summaryStaleRef.current = cachedSummary?.stale ?? true;

    const cachedDetails = peekGitDetailsCacheEntry(client, nextProjectRoot);
    setBranches(cachedDetails?.branches || null);
    setStatus(cachedDetails?.status || null);
    branchesRef.current = cachedDetails?.branches || null;
    statusRef.current = cachedDetails?.status || null;
    detailsLoadedRootRef.current = cachedDetails ? nextProjectRoot : null;
    detailsStaleRef.current = cachedDetails?.stale ?? true;
  }, [client]);

  const refreshClientInfo = useCallback(async () => {
    const cached = peekGitClientInfoCacheEntry(client);
    if (cached) {
      setClientInfo(cached.clientInfo);
    }
    const existingInflight = getGitClientInfoInflight(client);
    if (existingInflight) {
      setLoadingClientInfo(true);
      try {
        const resolved = await existingInflight;
        setClientInfo(resolved);
      } finally {
        setLoadingClientInfo(false);
      }
      return;
    }

    setLoadingClientInfo(true);
    try {
      const inflight = client.getGitClientInfo()
        .then((payload) => normalizeGitClientInfo(payload))
        .catch((err) => ({
          available: false,
          source: 'unknown' as const,
          path: 'git',
          version: null,
          error: err instanceof Error ? err.message : '加载 Git 客户端信息失败',
          bundledCandidates: [],
        }))
        .then((normalized) => {
          setGitClientInfoCacheEntry(client, normalized);
          return normalized;
        })
        .finally(() => {
          setGitClientInfoInflight(client, null);
        });
      setGitClientInfoInflight(client, inflight);
      setClientInfo(await inflight);
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

  const markSummaryStale = useCallback(() => {
    if (projectRoot) {
      markGitSummaryCacheStale(client, projectRoot);
    }
    summaryStaleRef.current = true;
  }, [client, projectRoot]);

  const markDetailsStale = useCallback(() => {
    if (projectRoot) {
      markGitDetailsCacheStale(client, projectRoot);
    }
    detailsStaleRef.current = true;
  }, [client, projectRoot]);

  const refreshSummary = useCallback(async (options?: { force?: boolean }) => {
    if (!projectRoot) {
      setSummary(null);
      summaryRef.current = null;
      summaryLoadedRootRef.current = null;
      summaryStaleRef.current = true;
      return;
    }
    const force = options?.force === true;
    if (!force) {
      const cached = peekGitSummaryCacheEntry(client, projectRoot);
      if (cached?.summary) {
        setSummary(cached.summary);
        summaryRef.current = cached.summary;
        summaryLoadedRootRef.current = projectRoot;
        summaryStaleRef.current = cached.stale;
      }
    }
    if (!force && !summaryStaleRef.current && summaryLoadedRootRef.current === projectRoot && summaryRef.current) {
      return;
    }
    const existingInflight = getGitSummaryInflight(client, projectRoot);
    if (existingInflight) {
      setLoadingSummary(true);
      setError(null);
      try {
        const normalized = await existingInflight;
        setSummary(normalized);
        summaryRef.current = normalized;
        summaryLoadedRootRef.current = projectRoot;
        summaryStaleRef.current = false;
      } catch (err) {
        setSummary(null);
        summaryRef.current = null;
        summaryLoadedRootRef.current = null;
        setError(err instanceof Error ? err.message : '加载 Git 状态失败');
      } finally {
        setLoadingSummary(false);
      }
      return;
    }
    setLoadingSummary(true);
    setError(null);
    try {
      const inflight = client.getGitSummary(projectRoot)
        .then((raw) => normalizeGitSummary(raw))
        .then((normalized) => {
          setGitSummaryCacheEntry(client, projectRoot, normalized);
          return normalized;
        })
        .finally(() => {
          setGitSummaryInflight(client, projectRoot, null);
        });
      setGitSummaryInflight(client, projectRoot, inflight);
      const normalized = await inflight;
      setSummary(normalized);
      summaryRef.current = normalized;
      summaryLoadedRootRef.current = projectRoot;
      summaryStaleRef.current = false;
    } catch (err) {
      setSummary(null);
      summaryRef.current = null;
      summaryLoadedRootRef.current = null;
      setError(err instanceof Error ? err.message : '加载 Git 状态失败');
    } finally {
      setLoadingSummary(false);
    }
  }, [client, projectRoot]);

  const loadDetails = useCallback(async (options?: { force?: boolean }) => {
    if (!projectRoot) {
      setBranches(null);
      setStatus(null);
      branchesRef.current = null;
      statusRef.current = null;
      detailsLoadedRootRef.current = null;
      detailsStaleRef.current = true;
      return;
    }
    const force = options?.force === true;
    if (!force) {
      const cached = peekGitDetailsCacheEntry(client, projectRoot);
      if (cached) {
        setBranches(cached.branches);
        setStatus(cached.status);
        branchesRef.current = cached.branches;
        statusRef.current = cached.status;
        detailsLoadedRootRef.current = projectRoot;
        detailsStaleRef.current = cached.stale;
      }
    }
    if (!force && !detailsStaleRef.current && detailsLoadedRootRef.current === projectRoot && branchesRef.current && statusRef.current) {
      return;
    }
    const existingInflight = getGitDetailsInflight(client, projectRoot);
    if (existingInflight) {
      setError(null);
      setLoadingBranches(true);
      setLoadingStatus(true);
      try {
        const resolved = await existingInflight;
        setBranches(resolved.branches);
        setStatus(resolved.status);
        branchesRef.current = resolved.branches;
        statusRef.current = resolved.status;
        detailsLoadedRootRef.current = projectRoot;
        detailsStaleRef.current = false;
      } catch (err) {
        branchesRef.current = null;
        statusRef.current = null;
        detailsLoadedRootRef.current = null;
        setError(err instanceof Error ? err.message : '加载 Git 详情失败');
      } finally {
        setLoadingBranches(false);
        setLoadingStatus(false);
      }
      return;
    }
    setError(null);
    setLoadingBranches(true);
    setLoadingStatus(true);
    try {
      const inflight = Promise.all([
        client.getGitBranches(projectRoot),
        client.getGitStatus(projectRoot),
      ])
        .then(([branchesRaw, statusRaw]) => ({
          branches: normalizeGitBranches(branchesRaw),
          status: normalizeGitStatus(statusRaw),
        }))
        .then((resolved) => {
          setGitDetailsCacheEntry(client, projectRoot, resolved);
          return resolved;
        })
        .finally(() => {
          setGitDetailsInflight(client, projectRoot, null);
        });
      setGitDetailsInflight(client, projectRoot, inflight);
      const resolved = await inflight;
      const normalizedBranches = resolved.branches;
      const normalizedStatus = resolved.status;
      setBranches(normalizedBranches);
      setStatus(normalizedStatus);
      branchesRef.current = normalizedBranches;
      statusRef.current = normalizedStatus;
      detailsLoadedRootRef.current = projectRoot;
      detailsStaleRef.current = false;
    } catch (err) {
      branchesRef.current = null;
      statusRef.current = null;
      detailsLoadedRootRef.current = null;
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
        summaryRef.current = result.summary;
        summaryLoadedRootRef.current = projectRoot || null;
        summaryStaleRef.current = false;
        if (projectRoot) {
          setGitSummaryCacheEntry(client, projectRoot, result.summary);
        }
      } else {
        await refreshSummary({ force: true });
      }
      detailsStaleRef.current = true;
      await loadDetails({ force: true });
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
  }, [loadDetails, onRepositoryChanged, projectRoot, refreshSummary]);

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
    discardFiles,
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
    open,
    clearCompare,
    setBranches,
    setStatus,
    setActionMessage,
    hydrateCachedState,
    projectRoot,
    refreshClientInfo,
    refreshSummary,
    markSummaryStale,
    markDetailsStale,
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
    markSummaryStale,
    markDetailsStale,
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
    discardFiles,
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
    markDetailsStale,
    markSummaryStale,
    pullCurrent,
    pushCurrent,
    refreshClientInfo,
    refreshSummary,
    stageFiles,
    status,
    summary,
    unstageFiles,
    discardFiles,
  ]);
};
