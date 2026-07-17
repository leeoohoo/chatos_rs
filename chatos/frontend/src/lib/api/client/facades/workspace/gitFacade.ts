// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import * as workspaceApi from '../../workspace';
import type {
  GitActionResponse,
  GitBranchesResponse,
  GitClientInfoResponse,
  GitCompareResponse,
  GitFileDiffResponse,
  GitStatusResponse,
  GitSummaryResponse,
} from '../../types';
import type ApiClient from '../../../client';
import { parseLocalConnectorProjectRoot } from '../../../localRuntime';

const isLocalRoot = (root?: string | null): boolean => Boolean(parseLocalConnectorProjectRoot(root));

export interface WorkspaceGitFacade {
  getGitClientInfo(): Promise<GitClientInfoResponse>;
  getGitSummary(root: string, preferredRepoRoot?: string, forceRefresh?: boolean): Promise<GitSummaryResponse>;
  getGitBranches(root: string, forceRefresh?: boolean): Promise<GitBranchesResponse>;
  getGitStatus(root: string, forceRefresh?: boolean): Promise<GitStatusResponse>;
  compareGitBranch(root: string, target: string): Promise<GitCompareResponse>;
  getGitDiff(data: { root: string; path: string; target?: string; staged?: boolean }): Promise<GitFileDiffResponse>;
  fetchGit(data: { root: string; remote?: string }): Promise<GitActionResponse>;
  pullGit(data: { root: string; mode?: 'ff-only' | 'rebase' | string }): Promise<GitActionResponse>;
  pushGit(data: { root: string; remote?: string; branch?: string; setUpstream?: boolean }): Promise<GitActionResponse>;
  checkoutGit(data: { root: string; branch?: string; remoteBranch?: string; createTracking?: boolean }): Promise<GitActionResponse>;
  createGitBranch(data: { root: string; name: string; startPoint?: string; checkout?: boolean }): Promise<GitActionResponse>;
  mergeGit(data: { root: string; branch: string; mode?: 'default' | 'no-ff' | 'ff-only' | string }): Promise<GitActionResponse>;
  stageGitPaths(data: { root: string; paths: string[] }): Promise<GitActionResponse>;
  unstageGitPaths(data: { root: string; paths: string[] }): Promise<GitActionResponse>;
  discardGitPaths(data: { root: string; paths: string[] }): Promise<GitActionResponse>;
  commitGit(data: { root: string; message: string; paths?: string[] }): Promise<GitActionResponse>;
}

export const workspaceGitFacade: WorkspaceGitFacade & ThisType<ApiClient> = {
  async getGitClientInfo() {
    if (typeof window !== 'undefined' && window.chatosLocalRuntime?.apiRequest) {
      return this.getLocalRuntimeClient().getGitClientInfo();
    }
    return workspaceApi.getGitClientInfo(this.getRequestFn());
  },
  async getGitSummary(root, preferredRepoRoot, forceRefresh) {
    if (isLocalRoot(root)) {
      return this.getLocalRuntimeClient().getGitSummary(preferredRepoRoot && isLocalRoot(preferredRepoRoot) ? preferredRepoRoot : root);
    }
    return workspaceApi.getGitSummary(this.getRequestFn(), root, preferredRepoRoot, forceRefresh);
  },
  async getGitBranches(root, forceRefresh) {
    if (isLocalRoot(root)) {
      return this.getLocalRuntimeClient().getGitBranches(root);
    }
    return workspaceApi.getGitBranches(this.getRequestFn(), root, forceRefresh);
  },
  async getGitStatus(root, forceRefresh) {
    if (isLocalRoot(root)) {
      return this.getLocalRuntimeClient().getGitStatus(root);
    }
    return workspaceApi.getGitStatus(this.getRequestFn(), root, forceRefresh);
  },
  async compareGitBranch(root, target) {
    if (isLocalRoot(root)) {
      return this.getLocalRuntimeClient().compareGitBranch(root, target);
    }
    return workspaceApi.compareGitBranch(this.getRequestFn(), root, target);
  },
  async getGitDiff(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().getGitDiff(data);
    }
    return workspaceApi.getGitDiff(this.getRequestFn(), data);
  },
  async fetchGit(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().fetchGit(data);
    }
    return workspaceApi.fetchGit(this.getRequestFn(), data);
  },
  async pullGit(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().pullGit(data);
    }
    return workspaceApi.pullGit(this.getRequestFn(), data);
  },
  async pushGit(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().pushGit(data);
    }
    return workspaceApi.pushGit(this.getRequestFn(), data);
  },
  async checkoutGit(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().checkoutGit(data);
    }
    return workspaceApi.checkoutGit(this.getRequestFn(), data);
  },
  async createGitBranch(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().createGitBranch(data);
    }
    return workspaceApi.createGitBranch(this.getRequestFn(), data);
  },
  async mergeGit(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().mergeGit(data);
    }
    return workspaceApi.mergeGit(this.getRequestFn(), data);
  },
  async stageGitPaths(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().stageGitPaths(data);
    }
    return workspaceApi.stageGitPaths(this.getRequestFn(), data);
  },
  async unstageGitPaths(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().unstageGitPaths(data);
    }
    return workspaceApi.unstageGitPaths(this.getRequestFn(), data);
  },
  async discardGitPaths(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().discardGitPaths(data);
    }
    return workspaceApi.discardGitPaths(this.getRequestFn(), data);
  },
  async commitGit(data) {
    if (isLocalRoot(data.root)) {
      return this.getLocalRuntimeClient().commitGit(data);
    }
    return workspaceApi.commitGit(this.getRequestFn(), data);
  },
};
