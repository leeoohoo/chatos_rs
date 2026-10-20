import { invoke } from "@tauri-apps/api/core";
import {
  projectGitDiffSchema,
  projectGitMutationResultSchema,
  projectGitSnapshotSchema,
} from "../domain/projectGit";
import type {
  ProjectGitBranchRequest,
  ProjectGitCommitRequest,
  ProjectGitCreateBranchRequest,
  ProjectGitDiffRequest,
  ProjectGitPathsRequest,
  ProjectGitPort,
  ProjectGitRemoteRequest,
  ProjectGitSaveRemoteRequest,
  ProjectGitScope,
} from "../ports/ProjectGitPort";

export class TauriProjectGitAdapter implements ProjectGitPort {
  async loadSnapshot(request: ProjectGitScope) {
    return projectGitSnapshotSchema.parse(await invoke("load_project_git_snapshot", { ...request }));
  }

  async loadDiff(request: ProjectGitDiffRequest) {
    return projectGitDiffSchema.parse(await invoke("load_project_git_diff", { ...request }));
  }

  async initializeRepository(request: ProjectGitScope) {
    return this.mutation("initialize_project_git_repository", request);
  }

  async stagePaths(request: ProjectGitPathsRequest) {
    return this.mutation("stage_project_git_paths", request);
  }

  async unstagePaths(request: ProjectGitPathsRequest) {
    return this.mutation("unstage_project_git_paths", request);
  }

  async commit(request: ProjectGitCommitRequest) {
    return this.mutation("commit_project_git", request);
  }

  async switchBranch(request: ProjectGitBranchRequest) {
    return this.mutation("switch_project_git_branch", request);
  }

  async createBranch(request: ProjectGitCreateBranchRequest) {
    return this.mutation("create_project_git_branch", request);
  }

  async mergeBranch(request: ProjectGitBranchRequest) {
    return this.mutation("merge_project_git_branch", request);
  }

  async saveRemote(request: ProjectGitSaveRemoteRequest) {
    return this.mutation("save_project_git_remote", request);
  }

  async removeRemote(request: ProjectGitRemoteRequest) {
    return this.mutation("remove_project_git_remote", request);
  }

  async pull(request: ProjectGitScope) {
    return this.mutation("pull_project_git", request);
  }

  async push(request: ProjectGitScope) {
    return this.mutation("push_project_git", request);
  }

  private async mutation(command: string, request: object) {
    return projectGitMutationResultSchema.parse(await invoke(command, { ...request }));
  }
}
