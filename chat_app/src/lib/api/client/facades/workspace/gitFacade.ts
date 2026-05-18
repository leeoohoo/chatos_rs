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

export interface WorkspaceGitFacade {
  getGitClientInfo(): Promise<GitClientInfoResponse>;
  getGitSummary(root: string, preferredRepoRoot?: string): Promise<GitSummaryResponse>;
  getGitBranches(root: string): Promise<GitBranchesResponse>;
  getGitStatus(root: string): Promise<GitStatusResponse>;
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
    return workspaceApi.getGitClientInfo(this.getRequestFn());
  },
  async getGitSummary(root, preferredRepoRoot) {
    return workspaceApi.getGitSummary(this.getRequestFn(), root, preferredRepoRoot);
  },
  async getGitBranches(root) {
    return workspaceApi.getGitBranches(this.getRequestFn(), root);
  },
  async getGitStatus(root) {
    return workspaceApi.getGitStatus(this.getRequestFn(), root);
  },
  async compareGitBranch(root, target) {
    return workspaceApi.compareGitBranch(this.getRequestFn(), root, target);
  },
  async getGitDiff(data) {
    return workspaceApi.getGitDiff(this.getRequestFn(), data);
  },
  async fetchGit(data) {
    return workspaceApi.fetchGit(this.getRequestFn(), data);
  },
  async pullGit(data) {
    return workspaceApi.pullGit(this.getRequestFn(), data);
  },
  async pushGit(data) {
    return workspaceApi.pushGit(this.getRequestFn(), data);
  },
  async checkoutGit(data) {
    return workspaceApi.checkoutGit(this.getRequestFn(), data);
  },
  async createGitBranch(data) {
    return workspaceApi.createGitBranch(this.getRequestFn(), data);
  },
  async mergeGit(data) {
    return workspaceApi.mergeGit(this.getRequestFn(), data);
  },
  async stageGitPaths(data) {
    return workspaceApi.stageGitPaths(this.getRequestFn(), data);
  },
  async unstageGitPaths(data) {
    return workspaceApi.unstageGitPaths(this.getRequestFn(), data);
  },
  async discardGitPaths(data) {
    return workspaceApi.discardGitPaths(this.getRequestFn(), data);
  },
  async commitGit(data) {
    return workspaceApi.commitGit(this.getRequestFn(), data);
  },
};
