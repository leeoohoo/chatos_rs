// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

export interface GitChangeCounts {
  staged: number;
  unstaged: number;
  untracked: number;
  conflicted: number;
}

export interface GitRepositoryCandidate {
  root: string;
  label: string;
  relativePath: string;
}

export interface GitClientInfo {
  available: boolean;
  source: 'env' | 'bundled' | 'system' | string;
  path: string;
  version?: string | null;
  error?: string | null;
  bundledCandidates: string[];
}

export interface GitSummary {
  isRepo: boolean;
  root?: string | null;
  worktreeRoot?: string | null;
  queryRoot?: string | null;
  resolvedRoot?: string | null;
  selectedRoot?: string | null;
  head?: string | null;
  currentBranch?: string | null;
  detached: boolean;
  upstream?: string | null;
  ahead: number;
  behind: number;
  dirty: boolean;
  operationState?: string | null;
  changes: GitChangeCounts;
  availableRepositories: GitRepositoryCandidate[];
}

export interface GitBranchInfo {
  name: string;
  shortName?: string | null;
  current: boolean;
  upstream?: string | null;
  remote?: string | null;
  trackedBy?: string | null;
  ahead: number;
  behind: number;
  lastCommit?: string | null;
  lastCommitSubject?: string | null;
}

export interface GitBranchesResult {
  current?: string | null;
  locals: GitBranchInfo[];
  remotes: GitBranchInfo[];
}

export interface GitStatusFile {
  path: string;
  oldPath?: string | null;
  status: string;
  staged: boolean;
  unstaged: boolean;
  conflicted: boolean;
}

export interface GitStatusResult {
  files: GitStatusFile[];
}

export interface GitDiffFile {
  path: string;
  oldPath?: string | null;
  status: string;
}

export interface GitCompareCommit {
  side: 'current' | 'target' | 'unknown' | string;
  hash: string;
  subject: string;
}

export interface GitCompareResult {
  current: string;
  target: string;
  files: GitDiffFile[];
  commits: GitCompareCommit[];
}

export interface GitFileDiff {
  path: string;
  target?: string | null;
  staged: boolean;
  patch: string;
}

export interface GitActionResult {
  success: boolean;
  summary?: GitSummary | null;
  stdout?: string | null;
  stderr?: string | null;
}
