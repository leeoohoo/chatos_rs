// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type {
  GitBranchesResult,
  GitClientInfo,
  GitRepositoryCandidate,
  GitStatusResult,
  GitSummary,
} from '../../../types';
import { useI18n } from '../../../i18n/I18nProvider';
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

const resolveActiveRepoRoot = (summary: GitSummary | null): string | null => (
  summary?.selectedRoot
  || summary?.resolvedRoot
  || summary?.root
  || summary?.worktreeRoot
  || null
);

const resolveAvailableRepositories = (summary: GitSummary | null): GitRepositoryCandidate[] => (
  summary?.availableRepositories || []
);

const mergeSummaryContext = (
  nextSummary: GitSummary,
  previousSummary: GitSummary | null,
  fallbackSelectedRoot: string | null,
): GitSummary => ({
  ...nextSummary,
  queryRoot: nextSummary.queryRoot ?? previousSummary?.queryRoot ?? null,
  resolvedRoot: nextSummary.resolvedRoot ?? fallbackSelectedRoot ?? previousSummary?.resolvedRoot ?? null,
  selectedRoot: nextSummary.selectedRoot ?? fallbackSelectedRoot ?? previousSummary?.selectedRoot ?? null,
  availableRepositories: nextSummary.availableRepositories.length > 0
    ? nextSummary.availableRepositories
    : previousSummary?.availableRepositories || [],
});

export const useProjectGit = ({
  client,
  projectRoot,
  open = false,
  enabled = true,
  onRepositoryChanged,
  onRepositorySelectionChange,
}: UseProjectGitOptions): UseProjectGitResult => {
  const { confirm } = useDialogService();
  const { t } = useI18n();
  const [preferredRepoRoot, setPreferredRepoRoot] = useState<string | null>(null);
  const [clientInfo, setClientInfo] = useState<GitClientInfo | null>(
    () => peekGitClientInfoCacheEntry(client)?.clientInfo || null,
  );
  const [summary, setSummary] = useState<GitSummary | null>(
    () => peekGitSummaryCacheEntry(client, projectRoot)?.summary || null,
  );
  const [branches, setBranches] = useState<GitBranchesResult | null>(null);
  const [status, setStatus] = useState<GitStatusResult | null>(null);
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
  const preferredRepoRootRef = useRef<string | null>(null);
  const activeRepoRootRef = useRef<string | null>(null);

  const activeRepoRoot = resolveActiveRepoRoot(summary);
  const availableRepositories = resolveAvailableRepositories(summary);
  activeRepoRootRef.current = activeRepoRoot;

  useEffect(() => {
    preferredRepoRootRef.current = null;
    setPreferredRepoRoot(null);
  }, [projectRoot]);

  const clearMessages = useCallback(() => {
    setError(null);
    setActionMessage(null);
  }, []);

  const hydrateCachedState = useCallback((nextProjectRoot: string) => {
    if (!enabled) {
      setClientInfo(null);
      setSummary(null);
      summaryRef.current = null;
      summaryLoadedRootRef.current = null;
      summaryStaleRef.current = true;
      setBranches(null);
      setStatus(null);
      branchesRef.current = null;
      statusRef.current = null;
      detailsLoadedRootRef.current = null;
      detailsStaleRef.current = true;
      return;
    }
    setClientInfo(peekGitClientInfoCacheEntry(client)?.clientInfo || null);
    const cachedSummary = peekGitSummaryCacheEntry(client, nextProjectRoot);
    const nextSummary = cachedSummary?.summary || null;
    setSummary(nextSummary);
    summaryRef.current = nextSummary;
    summaryLoadedRootRef.current = cachedSummary ? nextProjectRoot : null;
    summaryStaleRef.current = cachedSummary?.stale ?? true;

    const nextRepoRoot = resolveActiveRepoRoot(nextSummary);
    const cachedDetails = nextRepoRoot ? peekGitDetailsCacheEntry(client, nextRepoRoot) : null;
    setBranches(cachedDetails?.branches || null);
    setStatus(cachedDetails?.status || null);
    branchesRef.current = cachedDetails?.branches || null;
    statusRef.current = cachedDetails?.status || null;
    detailsLoadedRootRef.current = cachedDetails ? nextRepoRoot : null;
    detailsStaleRef.current = cachedDetails?.stale ?? true;
  }, [client, enabled]);

  const refreshClientInfo = useCallback(async () => {
    if (!enabled) {
      setClientInfo(null);
      return;
    }
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
          error: err instanceof Error ? err.message : t('git.error.clientInfoLoadFailed'),
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
        error: err instanceof Error ? err.message : t('git.error.clientInfoLoadFailed'),
        bundledCandidates: [],
      });
    } finally {
      setLoadingClientInfo(false);
    }
  }, [client, enabled, t]);

  const markSummaryStale = useCallback(() => {
    if (!enabled) {
      return;
    }
    if (projectRoot) {
      markGitSummaryCacheStale(client, projectRoot);
    }
    summaryStaleRef.current = true;
  }, [client, enabled, projectRoot]);

  const markDetailsStale = useCallback(() => {
    if (!enabled) {
      return;
    }
    const nextActiveRepoRoot = activeRepoRootRef.current;
    if (nextActiveRepoRoot) {
      markGitDetailsCacheStale(client, nextActiveRepoRoot);
    }
    detailsStaleRef.current = true;
  }, [client, enabled]);

  const refreshSummary = useCallback(async (options?: { force?: boolean; preferredRepoRoot?: string | null }) => {
    if (!enabled || !projectRoot) {
      setSummary(null);
      summaryRef.current = null;
      summaryLoadedRootRef.current = null;
      summaryStaleRef.current = true;
      return;
    }
    const effectivePreferredRepoRoot = options?.preferredRepoRoot !== undefined
      ? options.preferredRepoRoot
      : preferredRepoRootRef.current;
    const shouldForceRefresh = options?.force === true || summaryStaleRef.current;
    if (!shouldForceRefresh) {
      const cached = peekGitSummaryCacheEntry(client, projectRoot);
      if (cached?.summary) {
        setSummary(cached.summary);
        summaryRef.current = cached.summary;
        summaryLoadedRootRef.current = projectRoot;
        summaryStaleRef.current = cached.stale;
      }
    }
    if (!shouldForceRefresh && !summaryStaleRef.current && summaryLoadedRootRef.current === projectRoot && summaryRef.current) {
      return;
    }
    const existingInflight = shouldForceRefresh
      ? null
      : getGitSummaryInflight(client, projectRoot);
    if (existingInflight) {
      setLoadingSummary(true);
      setError(null);
      try {
        const normalized = await existingInflight;
        setSummary(normalized);
        summaryRef.current = normalized;
        activeRepoRootRef.current = resolveActiveRepoRoot(normalized);
        summaryLoadedRootRef.current = projectRoot;
        summaryStaleRef.current = false;
      } catch (err) {
        setSummary(null);
        summaryRef.current = null;
        activeRepoRootRef.current = null;
        summaryLoadedRootRef.current = null;
        setError(err instanceof Error ? err.message : t('git.error.statusLoadFailed'));
      } finally {
        setLoadingSummary(false);
      }
      return;
    }
    setLoadingSummary(true);
    setError(null);
    try {
      const summaryRequest = client.getGitSummary(
        projectRoot,
        effectivePreferredRepoRoot || undefined,
        shouldForceRefresh,
      )
        .then((raw) => normalizeGitSummary(raw))
        .then((normalized) => {
          setGitSummaryCacheEntry(client, projectRoot, normalized);
          return normalized;
        })
        .finally(() => {
          setGitSummaryInflight(client, projectRoot, null);
        });
      setGitSummaryInflight(client, projectRoot, summaryRequest);
      const normalized = await summaryRequest;
      setSummary(normalized);
      summaryRef.current = normalized;
      activeRepoRootRef.current = resolveActiveRepoRoot(normalized);
      summaryLoadedRootRef.current = projectRoot;
      summaryStaleRef.current = false;
    } catch (err) {
      setSummary(null);
      summaryRef.current = null;
      activeRepoRootRef.current = null;
      summaryLoadedRootRef.current = null;
      setError(err instanceof Error ? err.message : t('git.error.statusLoadFailed'));
    } finally {
      setLoadingSummary(false);
    }
  }, [client, enabled, projectRoot, t]);

  const loadDetails = useCallback(async (options?: { force?: boolean }) => {
    const latestRepoRoot = resolveActiveRepoRoot(summaryRef.current) || activeRepoRootRef.current || activeRepoRoot;
    if (!enabled || !latestRepoRoot) {
      setBranches(null);
      setStatus(null);
      branchesRef.current = null;
      statusRef.current = null;
      detailsLoadedRootRef.current = null;
      detailsStaleRef.current = true;
      return;
    }
    const shouldForceRefresh = options?.force === true || detailsStaleRef.current;
    if (!shouldForceRefresh) {
      const cached = peekGitDetailsCacheEntry(client, latestRepoRoot);
      if (cached) {
        setBranches(cached.branches);
        setStatus(cached.status);
        branchesRef.current = cached.branches;
        statusRef.current = cached.status;
        detailsLoadedRootRef.current = latestRepoRoot;
        detailsStaleRef.current = cached.stale;
      }
    }
    if (!shouldForceRefresh && !detailsStaleRef.current && detailsLoadedRootRef.current === latestRepoRoot && branchesRef.current && statusRef.current) {
      return;
    }
    const existingInflight = shouldForceRefresh
      ? null
      : getGitDetailsInflight(client, latestRepoRoot);
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
        detailsLoadedRootRef.current = latestRepoRoot;
        detailsStaleRef.current = false;
      } catch (err) {
        branchesRef.current = null;
        statusRef.current = null;
        detailsLoadedRootRef.current = null;
        setError(err instanceof Error ? err.message : t('git.error.detailsLoadFailed'));
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
        client.getGitBranches(latestRepoRoot, shouldForceRefresh),
        client.getGitStatus(latestRepoRoot, shouldForceRefresh),
      ])
        .then(([branchesRaw, statusRaw]) => ({
          branches: normalizeGitBranches(branchesRaw),
          status: normalizeGitStatus(statusRaw),
        }))
        .then((resolved) => {
          setGitDetailsCacheEntry(client, latestRepoRoot, resolved);
          return resolved;
        })
        .finally(() => {
          setGitDetailsInflight(client, latestRepoRoot, null);
        });
      setGitDetailsInflight(client, latestRepoRoot, inflight);
      const resolved = await inflight;
      setBranches(resolved.branches);
      setStatus(resolved.status);
      branchesRef.current = resolved.branches;
      statusRef.current = resolved.status;
      detailsLoadedRootRef.current = latestRepoRoot;
      detailsStaleRef.current = false;
    } catch (err) {
      branchesRef.current = null;
      statusRef.current = null;
      detailsLoadedRootRef.current = null;
      setError(err instanceof Error ? err.message : t('git.error.detailsLoadFailed'));
    } finally {
      setLoadingBranches(false);
      setLoadingStatus(false);
    }
  }, [activeRepoRoot, client, enabled, t]);

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
        const nextSummary = mergeSummaryContext(result.summary, summaryRef.current, activeRepoRoot);
        setSummary(nextSummary);
        summaryRef.current = nextSummary;
        summaryLoadedRootRef.current = projectRoot || null;
        summaryStaleRef.current = false;
        if (projectRoot) {
          setGitSummaryCacheEntry(client, projectRoot, nextSummary);
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
      setError(err instanceof Error ? err.message : t('git.error.actionFailed'));
      return false;
    } finally {
      setActionLoading(false);
    }
  }, [activeRepoRoot, client, loadDetails, onRepositoryChanged, projectRoot, refreshSummary, t]);

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
    projectRoot: activeRepoRoot,
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
    projectRoot: activeRepoRoot,
    summary,
    confirm,
    runAction,
    setError,
  });

  const selectRepository = useCallback(async (repoRoot: string | null) => {
    const normalized = typeof repoRoot === 'string' && repoRoot.trim() ? repoRoot.trim() : null;
    if ((normalized || null) === (preferredRepoRoot || null)) {
      return;
    }
    setPreferredRepoRoot(normalized);
    preferredRepoRootRef.current = normalized;
    clearMessages();
    clearCompare();
    clearFileDiff();
    setBranches(null);
    setStatus(null);
    branchesRef.current = null;
    statusRef.current = null;
    detailsLoadedRootRef.current = null;
    detailsStaleRef.current = true;
    summaryLoadedRootRef.current = null;
    summaryStaleRef.current = true;
    await onRepositorySelectionChange?.(normalized);
    await refreshSummary({ force: true, preferredRepoRoot: normalized });
    await loadDetails({ force: true });
  }, [
    clearCompare,
    clearFileDiff,
    clearMessages,
    loadDetails,
    onRepositorySelectionChange,
    preferredRepoRoot,
    refreshSummary,
  ]);

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
    activeRepoRoot,
    availableRepositories,
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
    selectRepository,
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
    activeRepoRoot,
    availableRepositories,
    branches,
    checkoutBranch,
    clearCompare,
    clearFileDiff,
    clearMessages,
    clientInfo,
    commitSelected,
    commitStaged,
    compareBranch,
    compareResult,
    createBranch,
    discardFiles,
    error,
    fetchRemote,
    fileDiff,
    loadDetails,
    loadFileDiff,
    loadingBranches,
    loadingClientInfo,
    loadingCompare,
    loadingDiff,
    loadingStatus,
    loadingSummary,
    markDetailsStale,
    markSummaryStale,
    mergeBranch,
    pullCurrent,
    pushCurrent,
    refreshClientInfo,
    refreshSummary,
    selectRepository,
    stageFiles,
    status,
    summary,
    unstageFiles,
  ]);
};
