import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import type {
  GitActionResult,
  GitBranchesResult,
  GitBranchInfo,
  GitClientInfo,
  GitCompareResult,
  GitFileDiff,
  GitStatusFile,
  GitStatusResult,
  GitSummary,
} from '../../../types';

export interface ProjectGitApiClient {
  getGitClientInfo: () => Promise<any>;
  getGitSummary: (root: string) => Promise<any>;
  getGitBranches: (root: string) => Promise<any>;
  getGitStatus: (root: string) => Promise<any>;
  compareGitBranch: (root: string, target: string) => Promise<any>;
  getGitDiff: (data: { root: string; path: string; target?: string; staged?: boolean }) => Promise<any>;
  fetchGit: (data: { root: string; remote?: string }) => Promise<any>;
  pullGit: (data: { root: string; mode?: 'ff-only' | 'rebase' | string }) => Promise<any>;
  pushGit: (data: { root: string; remote?: string; branch?: string; setUpstream?: boolean }) => Promise<any>;
  checkoutGit: (data: { root: string; branch?: string; remoteBranch?: string; createTracking?: boolean }) => Promise<any>;
  createGitBranch: (data: { root: string; name: string; startPoint?: string; checkout?: boolean }) => Promise<any>;
  mergeGit: (data: { root: string; branch: string; mode?: 'default' | 'no-ff' | 'ff-only' | string }) => Promise<any>;
  stageGitPaths: (data: { root: string; paths: string[] }) => Promise<any>;
  unstageGitPaths: (data: { root: string; paths: string[] }) => Promise<any>;
  commitGit: (data: { root: string; message: string; paths?: string[] }) => Promise<any>;
}

interface UseProjectGitOptions {
  client: ProjectGitApiClient;
  projectRoot: string;
  onRepositoryChanged?: () => Promise<void> | void;
}

export interface UseProjectGitResult {
  clientInfo: GitClientInfo | null;
  summary: GitSummary | null;
  branches: GitBranchesResult | null;
  status: GitStatusResult | null;
  compareResult: GitCompareResult | null;
  fileDiff: GitFileDiff | null;
  loadingSummary: boolean;
  loadingClientInfo: boolean;
  loadingBranches: boolean;
  loadingStatus: boolean;
  loadingCompare: boolean;
  loadingDiff: boolean;
  actionLoading: boolean;
  error: string | null;
  actionMessage: string | null;
  refreshClientInfo: () => Promise<void>;
  refreshSummary: () => Promise<void>;
  loadDetails: () => Promise<void>;
  fetchRemote: () => Promise<void>;
  pullCurrent: () => Promise<void>;
  pushCurrent: () => Promise<void>;
  checkoutBranch: (branch: GitBranchInfo) => Promise<void>;
  mergeBranch: (branch: GitBranchInfo) => Promise<void>;
  compareBranch: (branch: GitBranchInfo) => Promise<void>;
  loadFileDiff: (path: string, target?: string, staged?: boolean) => Promise<void>;
  clearCompare: () => void;
  clearFileDiff: () => void;
  createBranch: (name: string, startPoint?: string) => Promise<void>;
  stageFiles: (paths: string[]) => Promise<void>;
  unstageFiles: (paths: string[]) => Promise<void>;
  commitStaged: (message: string) => Promise<boolean>;
  commitSelected: (message: string, paths: string[]) => Promise<boolean>;
  clearMessages: () => void;
}

const toNumber = (value: unknown): number => (
  typeof value === 'number' && Number.isFinite(value) ? value : 0
);

const normalizeSummary = (raw: any): GitSummary => {
  const changes = raw?.changes || {};
  return {
    isRepo: Boolean(raw?.is_repo ?? raw?.isRepo),
    root: raw?.root ?? null,
    worktreeRoot: raw?.worktree_root ?? raw?.worktreeRoot ?? null,
    head: raw?.head ?? null,
    currentBranch: raw?.current_branch ?? raw?.currentBranch ?? null,
    detached: Boolean(raw?.detached),
    upstream: raw?.upstream ?? null,
    ahead: toNumber(raw?.ahead),
    behind: toNumber(raw?.behind),
    dirty: Boolean(raw?.dirty),
    operationState: raw?.operation_state ?? raw?.operationState ?? null,
    changes: {
      staged: toNumber(changes?.staged),
      unstaged: toNumber(changes?.unstaged),
      untracked: toNumber(changes?.untracked),
      conflicted: toNumber(changes?.conflicted),
    },
  };
};

const normalizeClientInfo = (raw: any): GitClientInfo => ({
  available: Boolean(raw?.available),
  source: String(raw?.source || 'system'),
  path: String(raw?.path || 'git'),
  version: raw?.version ?? null,
  error: raw?.error ?? null,
  bundledCandidates: Array.isArray(raw?.bundled_candidates)
    ? raw.bundled_candidates.map(String)
    : Array.isArray(raw?.bundledCandidates)
      ? raw.bundledCandidates.map(String)
      : [],
});

const normalizeBranch = (raw: any): GitBranchInfo => ({
  name: String(raw?.name || ''),
  shortName: raw?.short_name ?? raw?.shortName ?? null,
  current: Boolean(raw?.current),
  upstream: raw?.upstream ?? null,
  remote: raw?.remote ?? null,
  trackedBy: raw?.tracked_by ?? raw?.trackedBy ?? null,
  ahead: toNumber(raw?.ahead),
  behind: toNumber(raw?.behind),
  lastCommit: raw?.last_commit ?? raw?.lastCommit ?? null,
  lastCommitSubject: raw?.last_commit_subject ?? raw?.lastCommitSubject ?? null,
});

const normalizeBranches = (raw: any): GitBranchesResult => ({
  current: raw?.current ?? null,
  locals: Array.isArray(raw?.locals) ? raw.locals.map(normalizeBranch).filter((branch: GitBranchInfo) => branch.name) : [],
  remotes: Array.isArray(raw?.remotes) ? raw.remotes.map(normalizeBranch).filter((branch: GitBranchInfo) => branch.name) : [],
});

const normalizeStatusFile = (raw: any): GitStatusFile => ({
  path: String(raw?.path || ''),
  oldPath: raw?.old_path ?? raw?.oldPath ?? null,
  status: String(raw?.status || 'modified'),
  staged: Boolean(raw?.staged),
  unstaged: Boolean(raw?.unstaged),
  conflicted: Boolean(raw?.conflicted),
});

const normalizeStatus = (raw: any): GitStatusResult => ({
  files: Array.isArray(raw?.files) ? raw.files.map(normalizeStatusFile).filter((file: GitStatusFile) => file.path) : [],
});

const normalizeCompare = (raw: any): GitCompareResult => ({
  current: String(raw?.current || ''),
  target: String(raw?.target || ''),
  files: Array.isArray(raw?.files)
    ? raw.files.map((file: any) => ({
      path: String(file?.path || ''),
      oldPath: file?.old_path ?? file?.oldPath ?? null,
      status: String(file?.status || 'modified'),
    })).filter((file: { path: string }) => file.path)
    : [],
  commits: Array.isArray(raw?.commits)
    ? raw.commits.map((commit: any) => ({
      side: String(commit?.side || 'unknown'),
      hash: String(commit?.hash || ''),
      subject: String(commit?.subject || ''),
    })).filter((commit: { hash: string }) => commit.hash)
    : [],
});

const normalizeFileDiff = (raw: any): GitFileDiff => ({
  path: String(raw?.path || ''),
  target: raw?.target ?? null,
  staged: Boolean(raw?.staged),
  patch: String(raw?.patch || ''),
});

const normalizeAction = (raw: any): GitActionResult => ({
  success: raw?.success !== false,
  summary: raw?.summary ? normalizeSummary(raw.summary) : null,
  stdout: raw?.stdout ?? null,
  stderr: raw?.stderr ?? null,
});

const actionOutputMessage = (result: GitActionResult, fallback: string): string => (
  result.stdout || result.stderr || fallback
);

const actionErrorMessage = (result: GitActionResult, fallback: string): string => (
  result.stderr || result.stdout || fallback
);

export const useProjectGit = ({
  client,
  projectRoot,
  onRepositoryChanged,
}: UseProjectGitOptions): UseProjectGitResult => {
  const [clientInfo, setClientInfo] = useState<GitClientInfo | null>(null);
  const [summary, setSummary] = useState<GitSummary | null>(null);
  const [branches, setBranches] = useState<GitBranchesResult | null>(null);
  const [status, setStatus] = useState<GitStatusResult | null>(null);
  const [compareResult, setCompareResult] = useState<GitCompareResult | null>(null);
  const [fileDiff, setFileDiff] = useState<GitFileDiff | null>(null);
  const [loadingSummary, setLoadingSummary] = useState(false);
  const [loadingClientInfo, setLoadingClientInfo] = useState(false);
  const [loadingBranches, setLoadingBranches] = useState(false);
  const [loadingStatus, setLoadingStatus] = useState(false);
  const [loadingCompare, setLoadingCompare] = useState(false);
  const [loadingDiff, setLoadingDiff] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionMessage, setActionMessage] = useState<string | null>(null);
  const compareRequestIdRef = useRef(0);
  const diffRequestIdRef = useRef(0);

  const clearMessages = useCallback(() => {
    setError(null);
    setActionMessage(null);
  }, []);

  const clearCompare = useCallback(() => {
    compareRequestIdRef.current += 1;
    diffRequestIdRef.current += 1;
    setCompareResult(null);
    setFileDiff(null);
    setLoadingCompare(false);
    setLoadingDiff(false);
  }, []);

  const clearFileDiff = useCallback(() => {
    diffRequestIdRef.current += 1;
    setFileDiff(null);
    setLoadingDiff(false);
  }, []);

  const refreshClientInfo = useCallback(async () => {
    setLoadingClientInfo(true);
    try {
      setClientInfo(normalizeClientInfo(await client.getGitClientInfo()));
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
      setSummary(normalizeSummary(raw));
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
      setBranches(normalizeBranches(branchesRaw));
      setStatus(normalizeStatus(statusRaw));
    } catch (err) {
      setError(err instanceof Error ? err.message : '加载 Git 详情失败');
    } finally {
      setLoadingBranches(false);
      setLoadingStatus(false);
    }
  }, [client, projectRoot]);

  const runAction = useCallback(async (
    action: () => Promise<any>,
    fallbackMessage: string,
    repositoryChanging = false,
  ): Promise<boolean> => {
    setActionLoading(true);
    setError(null);
    setActionMessage(null);
    try {
      const result = normalizeAction(await action());
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

  const fetchRemote = useCallback(async () => {
    await runAction(
      () => client.fetchGit({ root: projectRoot, remote: 'origin' }),
      'Fetch 完成',
    );
  }, [client, projectRoot, runAction]);

  const pullCurrent = useCallback(async () => {
    await runAction(
      () => client.pullGit({ root: projectRoot, mode: 'ff-only' }),
      'Pull 完成',
      true,
    );
  }, [client, projectRoot, runAction]);

  const pushCurrent = useCallback(async () => {
    await runAction(
      () => client.pushGit({ root: projectRoot }),
      'Push 完成',
    );
  }, [client, projectRoot, runAction]);

  const checkoutBranch = useCallback(async (branch: GitBranchInfo) => {
    if (branch.current) return;
    if (summary?.dirty) {
      const confirmed = window.confirm('当前工作区有未提交改动，切换分支可能失败或影响改动。是否继续？');
      if (!confirmed) return;
    }
    const isRemote = Boolean(branch.remote);
    await runAction(
      () => client.checkoutGit({
        root: projectRoot,
        branch: isRemote ? branch.shortName || branch.name.split('/').slice(1).join('/') : branch.name,
        remoteBranch: isRemote ? branch.name : undefined,
        createTracking: isRemote && !branch.trackedBy,
      }),
      `已切换到 ${branch.shortName || branch.name}`,
      true,
    );
  }, [client, projectRoot, runAction, summary?.dirty]);

  const mergeBranch = useCallback(async (branch: GitBranchInfo) => {
    if (branch.current) return;
    const target = branch.name.trim();
    if (!target) return;
    if (summary?.operationState) {
      setError(`当前处于 ${summary.operationState} 状态，请先处理完再 Merge`);
      return;
    }
    if (summary?.detached) {
      setError('当前是 detached HEAD，无法从界面执行 Merge');
      return;
    }
    const current = summary?.currentBranch || 'HEAD';
    const targetLabel = branch.shortName || branch.name;
    const dirtyWarning = summary?.dirty
      ? '\n\n当前工作区有未提交改动，Merge 可能失败或产生冲突。建议先提交或暂存。是否继续？'
      : '';
    const confirmed = window.confirm(`确认将 ${targetLabel} 合并到当前分支 ${current} 吗？${dirtyWarning}`);
    if (!confirmed) return;
    await runAction(
      () => client.mergeGit({ root: projectRoot, branch: target, mode: 'default' }),
      `已将 ${targetLabel} 合并到 ${current}`,
      true,
    );
  }, [
    client,
    projectRoot,
    runAction,
    summary?.currentBranch,
    summary?.detached,
    summary?.dirty,
    summary?.operationState,
  ]);

  const createBranch = useCallback(async (name: string, startPoint?: string) => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError('分支名不能为空');
      return;
    }
    await runAction(
      () => client.createGitBranch({
        root: projectRoot,
        name: trimmed,
        startPoint,
        checkout: true,
      }),
      `已创建并切换到 ${trimmed}`,
      true,
    );
  }, [client, projectRoot, runAction]);

  const stageFiles = useCallback(async (paths: string[]) => {
    const validPaths = paths.map((path) => path.trim()).filter(Boolean);
    if (validPaths.length === 0) {
      setError('请选择要 Stage 的文件');
      return;
    }
    await runAction(
      () => client.stageGitPaths({ root: projectRoot, paths: validPaths }),
      'Stage 完成',
    );
  }, [client, projectRoot, runAction]);

  const unstageFiles = useCallback(async (paths: string[]) => {
    const validPaths = paths.map((path) => path.trim()).filter(Boolean);
    if (validPaths.length === 0) {
      setError('请选择要 Unstage 的文件');
      return;
    }
    await runAction(
      () => client.unstageGitPaths({ root: projectRoot, paths: validPaths }),
      'Unstage 完成',
    );
  }, [client, projectRoot, runAction]);

  const commitStaged = useCallback(async (message: string) => {
    const trimmed = message.trim();
    if (!trimmed) {
      setError('Commit message 不能为空');
      return false;
    }
    return runAction(
      () => client.commitGit({ root: projectRoot, message: trimmed }),
      'Commit staged 完成',
      true,
    );
  }, [client, projectRoot, runAction]);

  const commitSelected = useCallback(async (message: string, paths: string[]) => {
    const trimmed = message.trim();
    if (!trimmed) {
      setError('Commit message 不能为空');
      return false;
    }
    if (paths.length === 0) {
      setError('请至少选择一个文件');
      return false;
    }
    return runAction(
      () => client.commitGit({ root: projectRoot, message: trimmed, paths }),
      'Commit 完成',
      true,
    );
  }, [client, projectRoot, runAction]);

  useEffect(() => {
    setBranches(null);
    setStatus(null);
    clearCompare();
    setActionMessage(null);
    void refreshClientInfo();
    void refreshSummary();
  }, [clearCompare, refreshClientInfo, refreshSummary]);

  const compareBranch = useCallback(async (branch: GitBranchInfo) => {
    const target = branch.name;
    if (!target) return;
    const requestId = compareRequestIdRef.current + 1;
    compareRequestIdRef.current = requestId;
    setLoadingCompare(true);
    setError(null);
    setFileDiff(null);
    try {
      const raw = await client.compareGitBranch(projectRoot, target);
      if (compareRequestIdRef.current !== requestId) return;
      setCompareResult(normalizeCompare(raw));
    } catch (err) {
      if (compareRequestIdRef.current !== requestId) return;
      setCompareResult(null);
      setError(err instanceof Error ? err.message : '分支对比失败');
    } finally {
      if (compareRequestIdRef.current === requestId) {
        setLoadingCompare(false);
      }
    }
  }, [client, projectRoot]);

  const loadFileDiff = useCallback(async (path: string, target?: string, staged?: boolean) => {
    if (!path) return;
    const requestId = diffRequestIdRef.current + 1;
    diffRequestIdRef.current = requestId;
    setLoadingDiff(true);
    setError(null);
    try {
      const raw = await client.getGitDiff({ root: projectRoot, path, target, staged });
      if (diffRequestIdRef.current !== requestId) return;
      setFileDiff(normalizeFileDiff(raw));
    } catch (err) {
      if (diffRequestIdRef.current !== requestId) return;
      setFileDiff(null);
      setError(err instanceof Error ? err.message : '加载 diff 失败');
    } finally {
      if (diffRequestIdRef.current === requestId) {
        setLoadingDiff(false);
      }
    }
  }, [client, projectRoot]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      void refreshSummary();
    }, 15000);
    const handleFocus = () => {
      void refreshSummary();
    };
    window.addEventListener('focus', handleFocus);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('focus', handleFocus);
    };
  }, [refreshSummary]);

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
