export interface GitChangeCountsResponse {
  staged?: number;
  unstaged?: number;
  untracked?: number;
  conflicted?: number;
}

export interface GitSummaryResponse {
  is_repo?: boolean;
  isRepo?: boolean;
  root?: string | null;
  worktree_root?: string | null;
  worktreeRoot?: string | null;
  head?: string | null;
  current_branch?: string | null;
  currentBranch?: string | null;
  detached?: boolean;
  upstream?: string | null;
  ahead?: number;
  behind?: number;
  dirty?: boolean;
  operation_state?: string | null;
  operationState?: string | null;
  changes?: GitChangeCountsResponse;
}

export interface GitClientInfoResponse {
  available?: boolean;
  source?: string;
  path?: string;
  version?: string | null;
  error?: string | null;
  bundled_candidates?: string[];
  bundledCandidates?: string[];
}

export interface GitBranchInfoResponse {
  name?: string;
  short_name?: string | null;
  shortName?: string | null;
  current?: boolean;
  upstream?: string | null;
  remote?: string | null;
  tracked_by?: string | null;
  trackedBy?: string | null;
  ahead?: number;
  behind?: number;
  last_commit?: string | null;
  lastCommit?: string | null;
  last_commit_subject?: string | null;
  lastCommitSubject?: string | null;
}

export interface GitBranchesResponse {
  current?: string | null;
  locals?: GitBranchInfoResponse[];
  remotes?: GitBranchInfoResponse[];
}

export interface GitStatusFileResponse {
  path?: string;
  old_path?: string | null;
  oldPath?: string | null;
  status?: string;
  staged?: boolean;
  unstaged?: boolean;
  conflicted?: boolean;
}

export interface GitStatusResponse {
  files?: GitStatusFileResponse[];
}

export interface GitDiffFileResponse {
  path?: string;
  old_path?: string | null;
  oldPath?: string | null;
  status?: string;
}

export interface GitCompareCommitResponse {
  side?: string;
  hash?: string;
  subject?: string;
}

export interface GitCompareResponse {
  current?: string;
  target?: string;
  files?: GitDiffFileResponse[];
  commits?: GitCompareCommitResponse[];
}

export interface GitFileDiffResponse {
  path?: string;
  target?: string | null;
  staged?: boolean;
  patch?: string;
}

export interface GitActionResponse {
  success?: boolean;
  summary?: GitSummaryResponse;
  stdout?: string | null;
  stderr?: string | null;
}
