// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import { buildQuery } from '../shared';
import type {
  GitActionResponse,
  GitBranchesResponse,
  GitClientInfoResponse,
  GitCompareResponse,
  GitFileDiffResponse,
  GitStatusResponse,
  GitSummaryResponse,
} from '../types';
import type { ApiRequestFn } from './common';

export const getGitSummary = (
  request: ApiRequestFn,
  root: string,
  preferredRepoRoot?: string,
  forceRefresh?: boolean,
): Promise<GitSummaryResponse> => {
  return request<GitSummaryResponse>(`/git/summary${buildQuery({
    root,
    preferred_repo_root: preferredRepoRoot,
    force_refresh: forceRefresh,
  })}`);
};

export const getGitClientInfo = (
  request: ApiRequestFn,
): Promise<GitClientInfoResponse> => {
  return request<GitClientInfoResponse>('/git/client');
};

export const getGitBranches = (
  request: ApiRequestFn,
  root: string,
  forceRefresh?: boolean,
): Promise<GitBranchesResponse> => {
  return request<GitBranchesResponse>(`/git/branches${buildQuery({ root, force_refresh: forceRefresh })}`);
};

export const getGitStatus = (
  request: ApiRequestFn,
  root: string,
  forceRefresh?: boolean,
): Promise<GitStatusResponse> => {
  return request<GitStatusResponse>(`/git/status${buildQuery({ root, force_refresh: forceRefresh })}`);
};

export const compareGitBranch = (
  request: ApiRequestFn,
  root: string,
  target: string,
): Promise<GitCompareResponse> => {
  return request<GitCompareResponse>(`/git/compare${buildQuery({ root, target })}`);
};

export const getGitDiff = (
  request: ApiRequestFn,
  data: { root: string; path: string; target?: string; staged?: boolean },
): Promise<GitFileDiffResponse> => {
  return request<GitFileDiffResponse>(`/git/diff${buildQuery({
    root: data.root,
    path: data.path,
    target: data.target,
    staged: data.staged,
  })}`);
};

export const fetchGit = (
  request: ApiRequestFn,
  data: { root: string; remote?: string },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/fetch', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      remote: data.remote,
    }),
  });
};

export const pullGit = (
  request: ApiRequestFn,
  data: { root: string; mode?: 'ff-only' | 'rebase' | string },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/pull', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      mode: data.mode,
    }),
  });
};

export const pushGit = (
  request: ApiRequestFn,
  data: { root: string; remote?: string; branch?: string; setUpstream?: boolean },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/push', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      remote: data.remote,
      branch: data.branch,
      set_upstream: data.setUpstream,
    }),
  });
};

export const checkoutGit = (
  request: ApiRequestFn,
  data: { root: string; branch?: string; remoteBranch?: string; createTracking?: boolean },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/checkout', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      branch: data.branch,
      remote_branch: data.remoteBranch,
      create_tracking: data.createTracking,
    }),
  });
};

export const createGitBranch = (
  request: ApiRequestFn,
  data: { root: string; name: string; startPoint?: string; checkout?: boolean },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/branch', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      name: data.name,
      start_point: data.startPoint,
      checkout: data.checkout,
    }),
  });
};

export const mergeGit = (
  request: ApiRequestFn,
  data: { root: string; branch: string; mode?: 'default' | 'no-ff' | 'ff-only' | string },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/merge', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      branch: data.branch,
      mode: data.mode,
    }),
  });
};

export const stageGitPaths = (
  request: ApiRequestFn,
  data: { root: string; paths: string[] },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/stage', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      paths: data.paths,
    }),
  });
};

export const unstageGitPaths = (
  request: ApiRequestFn,
  data: { root: string; paths: string[] },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/unstage', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      paths: data.paths,
    }),
  });
};

export const commitGit = (
  request: ApiRequestFn,
  data: { root: string; message: string; paths?: string[] },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/commit', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      message: data.message,
      paths: data.paths,
    }),
  });
};

export const discardGitPaths = (
  request: ApiRequestFn,
  data: { root: string; paths: string[] },
): Promise<GitActionResponse> => {
  return request<GitActionResponse>('/git/discard', {
    method: 'POST',
    body: JSON.stringify({
      root: data.root,
      paths: data.paths,
    }),
  });
};
