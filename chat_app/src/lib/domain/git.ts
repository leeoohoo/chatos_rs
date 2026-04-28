import type {
  GitActionResponse,
  GitBranchesResponse,
  GitBranchInfoResponse,
  GitClientInfoResponse,
  GitCompareCommitResponse,
  GitCompareResponse,
  GitDiffFileResponse,
  GitFileDiffResponse,
  GitStatusFileResponse,
  GitStatusResponse,
  GitSummaryResponse,
} from '../api/client/types';
import type {
  GitActionResult,
  GitBranchesResult,
  GitBranchInfo,
  GitClientInfo,
  GitCompareResult,
  GitDiffFile,
  GitFileDiff,
  GitStatusFile,
  GitStatusResult,
  GitSummary,
} from '../../types';

const toNumber = (value: unknown): number => (
  typeof value === 'number' && Number.isFinite(value) ? value : 0
);

export const normalizeGitSummary = (raw: GitSummaryResponse | null | undefined): GitSummary => {
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

export const normalizeGitClientInfo = (raw: GitClientInfoResponse | null | undefined): GitClientInfo => ({
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

export const normalizeGitBranch = (raw: GitBranchInfoResponse | null | undefined): GitBranchInfo => ({
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

export const normalizeGitBranches = (raw: GitBranchesResponse | null | undefined): GitBranchesResult => ({
  current: raw?.current ?? null,
  locals: Array.isArray(raw?.locals) ? raw.locals.map(normalizeGitBranch).filter((branch: GitBranchInfo) => branch.name) : [],
  remotes: Array.isArray(raw?.remotes) ? raw.remotes.map(normalizeGitBranch).filter((branch: GitBranchInfo) => branch.name) : [],
});

export const normalizeGitStatusFile = (raw: GitStatusFileResponse | null | undefined): GitStatusFile => ({
  path: String(raw?.path || ''),
  oldPath: raw?.old_path ?? raw?.oldPath ?? null,
  status: String(raw?.status || 'modified'),
  staged: Boolean(raw?.staged),
  unstaged: Boolean(raw?.unstaged),
  conflicted: Boolean(raw?.conflicted),
});

export const normalizeGitStatus = (raw: GitStatusResponse | null | undefined): GitStatusResult => ({
  files: Array.isArray(raw?.files) ? raw.files.map(normalizeGitStatusFile).filter((file: GitStatusFile) => file.path) : [],
});

export const normalizeGitDiffFile = (file: GitDiffFileResponse | null | undefined): GitDiffFile => ({
  path: String(file?.path || ''),
  oldPath: file?.old_path ?? file?.oldPath ?? null,
  status: String(file?.status || 'modified'),
});

export const normalizeGitCompareCommit = (
  commit: GitCompareCommitResponse | null | undefined,
) => ({
  side: String(commit?.side || 'unknown'),
  hash: String(commit?.hash || ''),
  subject: String(commit?.subject || ''),
});

export const normalizeGitCompare = (raw: GitCompareResponse | null | undefined): GitCompareResult => ({
  current: String(raw?.current || ''),
  target: String(raw?.target || ''),
  files: Array.isArray(raw?.files)
    ? raw.files.map(normalizeGitDiffFile).filter((file: GitDiffFile) => file.path)
    : [],
  commits: Array.isArray(raw?.commits)
    ? raw.commits.map(normalizeGitCompareCommit).filter((commit: { hash: string }) => commit.hash)
    : [],
});

export const normalizeGitFileDiff = (raw: GitFileDiffResponse | null | undefined): GitFileDiff => ({
  path: String(raw?.path || ''),
  target: raw?.target ?? null,
  staged: Boolean(raw?.staged),
  patch: String(raw?.patch || ''),
});

export const normalizeGitAction = (raw: GitActionResponse | null | undefined): GitActionResult => ({
  success: raw?.success !== false,
  summary: raw?.summary ? normalizeGitSummary(raw.summary) : null,
  stdout: raw?.stdout ?? null,
  stderr: raw?.stderr ?? null,
});
