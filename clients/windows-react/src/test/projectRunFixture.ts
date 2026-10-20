import type {
  ProjectRunAnalysisSnapshot,
  ProjectRunState,
} from "../features/project-run/domain/projectRun";
import type { ProjectRunPort } from "../features/project-run/ports/ProjectRunPort";

export function projectRunAnalysis(
  overrides: Partial<ProjectRunAnalysisSnapshot> = {},
): ProjectRunAnalysisSnapshot {
  return {
    catalog: {
      projectId: "project-1",
      status: "ready",
      defaultTargetId: "target-1",
      targets: [{
        id: "target-1",
        label: "Web 开发服务器",
        kind: "node-script",
        language: "JavaScript",
        cwd: ".",
        command: "npm run dev",
        source: "package.json",
        isDefault: true,
        entrypoint: null,
        manifestPath: "package.json",
        requiredToolchains: ["node", "npm"],
      }],
      errorMessage: null,
    },
    environment: {
      toolchainOptions: {
        node: [{ id: "node-system", kind: "node", label: "Node.js", version: "22.0.0", source: "PATH", isDefault: true }],
        npm: [{ id: "npm-system", kind: "npm", label: "npm", version: "10.0.0", source: "PATH", isDefault: true }],
      },
      configurationFiles: [{ id: "config-1", kind: "package", label: "package.json", path: "package.json", preview: "{\n  \"scripts\": {}\n}", source: "analyzer" }],
      validationIssues: [],
      selectedToolchains: {},
      customToolchains: {},
      environmentVariables: [{ key: "API_TOKEN", hasValue: true }],
      terminalUiEnabled: false,
    },
    ...overrides,
  };
}

export function projectRunState(overrides: Partial<ProjectRunState> = {}): ProjectRunState {
  return {
    projectId: "project-1",
    status: "idle",
    isBusy: false,
    isRunning: false,
    revision: 1,
    instances: [],
    ...overrides,
  };
}

export function runningProjectState(overrides: Partial<ProjectRunState> = {}): ProjectRunState {
  return projectRunState({
    status: "running",
    isRunning: true,
    revision: 2,
    instances: [{
      id: "instance-1",
      targetId: "target-1",
      name: "Web 开发服务器",
      cwd: ".",
      status: "running",
      isBusy: false,
      isRunning: true,
      log: "ready on http://localhost:3000\n",
      logTruncated: false,
      startedAtUnixMs: 1_798_000_000_000,
      exitCode: null,
    }],
    ...overrides,
  });
}

export function stubProjectRunPort(
  analysis = projectRunAnalysis(),
  state = projectRunState(),
): ProjectRunPort {
  return {
    analyze: async () => analysis,
    loadState: async () => state,
    pollChanges: async () => new Promise(() => undefined),
    start: async () => runningProjectState(),
    stop: async () => projectRunState({ revision: 3 }),
    deleteInstance: async () => projectRunState({ revision: 4 }),
    setDefaultTarget: async () => analysis,
    updateEnvironment: async () => analysis,
    removeProject: async (request) => ({
      projectId: request.projectId,
      removedRevision: 1,
      registryRevision: 2,
      directoryPreserved: true,
      preferencesCleaned: true,
    }),
    loadFavorites: async () => ({ projectIds: [] }),
    setFavorite: async (request) => ({ projectIds: request.isFavorite ? [request.projectId] : [] }),
  };
}
