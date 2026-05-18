import type {
  GitActionResponse,
  GitBranchesResponse,
  GitClientInfoResponse,
  GitCompareResponse,
  GitFileDiffResponse,
  GitStatusResponse,
  GitSummaryResponse,
} from '../../../lib/api/client/types';
import type {
  GitBranchInfo,
  GitBranchesResult,
  GitClientInfo,
  GitCompareResult,
  GitFileDiff,
  GitStatusResult,
  GitSummary,
} from '../../../types';

export interface ProjectGitApiClient {
  getGitClientInfo: () => Promise<GitClientInfoResponse>;
  getGitSummary: (root: string) => Promise<GitSummaryResponse>;
  getGitBranches: (root: string) => Promise<GitBranchesResponse>;
  getGitStatus: (root: string) => Promise<GitStatusResponse>;
  compareGitBranch: (root: string, target: string) => Promise<GitCompareResponse>;
  getGitDiff: (data: { root: string; path: string; target?: string; staged?: boolean }) => Promise<GitFileDiffResponse>;
  fetchGit: (data: { root: string; remote?: string }) => Promise<GitActionResponse>;
  pullGit: (data: { root: string; mode?: 'ff-only' | 'rebase' | string }) => Promise<GitActionResponse>;
  pushGit: (data: { root: string; remote?: string; branch?: string; setUpstream?: boolean }) => Promise<GitActionResponse>;
  checkoutGit: (data: { root: string; branch?: string; remoteBranch?: string; createTracking?: boolean }) => Promise<GitActionResponse>;
  createGitBranch: (data: { root: string; name: string; startPoint?: string; checkout?: boolean }) => Promise<GitActionResponse>;
  mergeGit: (data: { root: string; branch: string; mode?: 'default' | 'no-ff' | 'ff-only' | string }) => Promise<GitActionResponse>;
  stageGitPaths: (data: { root: string; paths: string[] }) => Promise<GitActionResponse>;
  unstageGitPaths: (data: { root: string; paths: string[] }) => Promise<GitActionResponse>;
  discardGitPaths: (data: { root: string; paths: string[] }) => Promise<GitActionResponse>;
  commitGit: (data: { root: string; message: string; paths?: string[] }) => Promise<GitActionResponse>;
}

export interface UseProjectGitOptions {
  client: ProjectGitApiClient;
  projectRoot: string;
  open?: boolean;
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
  refreshSummary: (options?: { force?: boolean }) => Promise<void>;
  loadDetails: (options?: { force?: boolean }) => Promise<void>;
  markSummaryStale: () => void;
  markDetailsStale: () => void;
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
  discardFiles: (paths: string[]) => Promise<void>;
  commitStaged: (message: string) => Promise<boolean>;
  commitSelected: (message: string, paths: string[]) => Promise<boolean>;
  clearMessages: () => void;
}
