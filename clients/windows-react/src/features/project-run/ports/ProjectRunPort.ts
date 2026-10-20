import type {
  ProjectFavoriteSnapshot,
  ProjectRemovalReceipt,
  ProjectRunAnalysisSnapshot,
  ProjectRunChangeSnapshot,
  ProjectRunState,
} from "../domain/projectRun";

export interface ProjectRunScope {
  ownerUserId: string;
  projectId: string;
}

export interface ProjectRunPollRequest extends ProjectRunScope {
  afterRevision: number;
  timeoutMs: number;
}

export interface ProjectRunTargetRequest extends ProjectRunScope {
  targetId: string;
}

export interface ProjectRunInstanceRequest extends ProjectRunScope {
  instanceId: string;
}

export interface ProjectRunEnvironmentMutation {
  key: string;
  value: string | null;
}

export interface ProjectRunEnvironmentRequest extends ProjectRunScope {
  selectedToolchains: Record<string, string>;
  environmentVariables: ProjectRunEnvironmentMutation[];
}

export interface ProjectFavoriteScope {
  ownerUserId: string;
}

export interface SetProjectFavoriteRequest extends ProjectRunScope {
  isFavorite: boolean;
}

export interface ProjectRunPort {
  analyze(request: ProjectRunScope): Promise<ProjectRunAnalysisSnapshot>;
  loadState(request: ProjectRunScope): Promise<ProjectRunState>;
  pollChanges(request: ProjectRunPollRequest): Promise<ProjectRunChangeSnapshot>;
  start(request: ProjectRunTargetRequest): Promise<ProjectRunState>;
  stop(request: ProjectRunInstanceRequest): Promise<ProjectRunState>;
  deleteInstance(request: ProjectRunInstanceRequest): Promise<ProjectRunState>;
  setDefaultTarget(request: ProjectRunTargetRequest): Promise<ProjectRunAnalysisSnapshot>;
  updateEnvironment(request: ProjectRunEnvironmentRequest): Promise<ProjectRunAnalysisSnapshot>;
  removeProject(request: ProjectRunScope): Promise<ProjectRemovalReceipt>;
  loadFavorites(request: ProjectFavoriteScope): Promise<ProjectFavoriteSnapshot>;
  setFavorite(request: SetProjectFavoriteRequest): Promise<ProjectFavoriteSnapshot>;
}
