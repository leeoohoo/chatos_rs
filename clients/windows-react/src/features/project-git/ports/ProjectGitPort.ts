import type {
  ProjectGitDiff,
  ProjectGitMutationResult,
  ProjectGitSnapshot,
} from "../domain/projectGit";

export interface ProjectGitScope {
  ownerUserId: string;
  projectId: string;
}

export interface ProjectGitDiffRequest extends ProjectGitScope {
  path: string;
  staged: boolean;
}

export interface ProjectGitPathsRequest extends ProjectGitScope {
  paths: string[];
}

export interface ProjectGitCommitRequest extends ProjectGitScope {
  message: string;
}

export interface ProjectGitBranchRequest extends ProjectGitScope {
  branch: string;
}

export interface ProjectGitCreateBranchRequest extends ProjectGitScope {
  name: string;
  switchToBranch: boolean;
}

export interface ProjectGitSaveRemoteRequest extends ProjectGitScope {
  originalName: string | null;
  name: string;
  url: string | null;
}

export interface ProjectGitRemoteRequest extends ProjectGitScope {
  name: string;
}

export interface ProjectGitPort {
  loadSnapshot(request: ProjectGitScope): Promise<ProjectGitSnapshot>;
  loadDiff(request: ProjectGitDiffRequest): Promise<ProjectGitDiff>;
  initializeRepository(request: ProjectGitScope): Promise<ProjectGitMutationResult>;
  stagePaths(request: ProjectGitPathsRequest): Promise<ProjectGitMutationResult>;
  unstagePaths(request: ProjectGitPathsRequest): Promise<ProjectGitMutationResult>;
  commit(request: ProjectGitCommitRequest): Promise<ProjectGitMutationResult>;
  switchBranch(request: ProjectGitBranchRequest): Promise<ProjectGitMutationResult>;
  createBranch(request: ProjectGitCreateBranchRequest): Promise<ProjectGitMutationResult>;
  mergeBranch(request: ProjectGitBranchRequest): Promise<ProjectGitMutationResult>;
  saveRemote(request: ProjectGitSaveRemoteRequest): Promise<ProjectGitMutationResult>;
  removeRemote(request: ProjectGitRemoteRequest): Promise<ProjectGitMutationResult>;
  pull(request: ProjectGitScope): Promise<ProjectGitMutationResult>;
  push(request: ProjectGitScope): Promise<ProjectGitMutationResult>;
}
