import { invoke } from "@tauri-apps/api/core";
import {
  projectFavoriteSnapshotSchema,
  projectRemovalReceiptSchema,
  projectRunAnalysisSnapshotSchema,
  projectRunChangeSnapshotSchema,
  projectRunStateSchema,
} from "../domain/projectRun";
import type {
  ProjectFavoriteScope,
  ProjectRunEnvironmentRequest,
  ProjectRunInstanceRequest,
  ProjectRunPollRequest,
  ProjectRunPort,
  ProjectRunScope,
  ProjectRunTargetRequest,
  SetProjectFavoriteRequest,
} from "../ports/ProjectRunPort";

export class TauriProjectRunAdapter implements ProjectRunPort {
  async analyze(request: ProjectRunScope) {
    return projectRunAnalysisSnapshotSchema.parse(await invoke("analyze_project_run", { ...request }));
  }

  async loadState(request: ProjectRunScope) {
    return projectRunStateSchema.parse(await invoke("load_project_run_state", { ...request }));
  }

  async pollChanges(request: ProjectRunPollRequest) {
    return projectRunChangeSnapshotSchema.parse(await invoke("poll_project_run_changes", { ...request }));
  }

  async start(request: ProjectRunTargetRequest) {
    return projectRunStateSchema.parse(await invoke("start_project_run", { ...request }));
  }

  async stop(request: ProjectRunInstanceRequest) {
    return projectRunStateSchema.parse(await invoke("stop_project_run", { ...request }));
  }

  async deleteInstance(request: ProjectRunInstanceRequest) {
    return projectRunStateSchema.parse(await invoke("delete_project_run_instance", { ...request }));
  }

  async setDefaultTarget(request: ProjectRunTargetRequest) {
    return projectRunAnalysisSnapshotSchema.parse(await invoke("set_project_run_default_target", { ...request }));
  }

  async updateEnvironment(request: ProjectRunEnvironmentRequest) {
    return projectRunAnalysisSnapshotSchema.parse(await invoke("update_project_run_environment", { ...request }));
  }

  async removeProject(request: ProjectRunScope) {
    return projectRemovalReceiptSchema.parse(await invoke("remove_local_project", { ...request }));
  }

  async loadFavorites(request: ProjectFavoriteScope) {
    return projectFavoriteSnapshotSchema.parse(await invoke("load_project_favorites", { ...request }));
  }

  async setFavorite(request: SetProjectFavoriteRequest) {
    return projectFavoriteSnapshotSchema.parse(await invoke("set_project_favorite", { ...request }));
  }
}
