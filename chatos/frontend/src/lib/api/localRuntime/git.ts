// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import type {
  GitActionResponse,
  GitBranchesResponse,
  GitClientInfoResponse,
  GitCompareResponse,
  GitFileDiffResponse,
  GitStatusResponse,
  GitSummaryResponse,
} from '../client/types';
import { requestLocalRuntime } from './bridge';

const rootQuery = (root: string): string => `?root=${encodeURIComponent(root)}`;

export const getLocalGitClientInfo = (): Promise<GitClientInfoResponse> =>
  requestLocalRuntime('/api/local/runtime/git/client');

export const getLocalGitSummary = (root: string): Promise<GitSummaryResponse> =>
  requestLocalRuntime(`/api/local/runtime/git/summary${rootQuery(root)}`);

export const getLocalGitBranches = (root: string): Promise<GitBranchesResponse> =>
  requestLocalRuntime(`/api/local/runtime/git/branches${rootQuery(root)}`);

export const getLocalGitStatus = (root: string): Promise<GitStatusResponse> =>
  requestLocalRuntime(`/api/local/runtime/git/status${rootQuery(root)}`);

export const compareLocalGitBranch = (
  root: string,
  target: string,
): Promise<GitCompareResponse> =>
  requestLocalRuntime(
    `/api/local/runtime/git/compare${rootQuery(root)}&target=${encodeURIComponent(target)}`,
  );

export const getLocalGitDiff = (data: {
  root: string;
  path: string;
  target?: string;
  staged?: boolean;
}): Promise<GitFileDiffResponse> => {
  const params = new URLSearchParams({ root: data.root, path: data.path });
  if (data.target) params.set('target', data.target);
  if (data.staged !== undefined) params.set('staged', String(data.staged));
  return requestLocalRuntime(`/api/local/runtime/git/diff?${params.toString()}`);
};

const gitAction = (
  action: string,
  payload: Record<string, unknown>,
): Promise<GitActionResponse> =>
  requestLocalRuntime(`/api/local/runtime/git/${action}`, {
    method: 'POST',
    body: JSON.stringify(payload),
  });

export const fetchLocalGit = (data: { root: string; remote?: string }) =>
  gitAction('fetch', data);

export const pullLocalGit = (data: { root: string; mode?: string }) =>
  gitAction('pull', data);

export const pushLocalGit = (data: {
  root: string;
  remote?: string;
  branch?: string;
  setUpstream?: boolean;
}) => gitAction('push', { ...data, set_upstream: data.setUpstream });

export const checkoutLocalGit = (data: {
  root: string;
  branch?: string;
  remoteBranch?: string;
  createTracking?: boolean;
}) => gitAction('checkout', {
  ...data,
  remote_branch: data.remoteBranch,
  create_tracking: data.createTracking,
});

export const createLocalGitBranch = (data: {
  root: string;
  name: string;
  startPoint?: string;
  checkout?: boolean;
}) => gitAction('branch', { ...data, start_point: data.startPoint });

export const mergeLocalGit = (data: { root: string; branch: string; mode?: string }) =>
  gitAction('merge', data);

export const stageLocalGitPaths = (data: { root: string; paths: string[] }) =>
  gitAction('stage', data);

export const unstageLocalGitPaths = (data: { root: string; paths: string[] }) =>
  gitAction('unstage', data);

export const discardLocalGitPaths = (data: { root: string; paths: string[] }) =>
  gitAction('discard', data);

export const commitLocalGit = (data: { root: string; message: string; paths?: string[] }) =>
  gitAction('commit', data);
