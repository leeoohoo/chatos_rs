import { create } from "zustand";
import {
  environmentDrafts,
  selectedProjectRunTarget,
  type ProjectRunAnalysisSnapshot,
  type ProjectRunCatalog,
  type ProjectRunEnvironment,
  type ProjectRunEnvironmentDraft,
  type ProjectRunState,
} from "../domain/projectRun";
import type { ProjectRunPort } from "../ports/ProjectRunPort";

export type ProjectRunMutationKind =
  | "analyze"
  | "defaultTarget"
  | "environment"
  | "start"
  | "stop"
  | "deleteInstance"
  | "favorite"
  | "removeProject";

export type ProjectRunNotice =
  | "analyzed"
  | "defaultTargetSaved"
  | "environmentSaved"
  | "instanceStarted"
  | "stopRequested"
  | "instanceDeleted"
  | "favoriteSaved";

interface MutationIdentity {
  id: number;
  kind: ProjectRunMutationKind;
}

export interface ProjectRunStateStore {
  ownerUserId: string | null;
  projectId: string | null;
  catalog: ProjectRunCatalog | null;
  environment: ProjectRunEnvironment | null;
  runState: ProjectRunState | null;
  selectedToolchains: Record<string, string>;
  environmentVariables: ProjectRunEnvironmentDraft[];
  isFavorite: boolean;
  isLoading: boolean;
  hasLoaded: boolean;
  isMutating: boolean;
  mutationKind: ProjectRunMutationKind | null;
  errorMessage: string | null;
  monitorWarning: string | null;
  notice: ProjectRunNotice | null;
  generation: number;
  requestSequence: number;
  draftSequence: number;
  activeLoadRequest: number | null;
  activeMonitorRequest: number | null;
  activeMutationRequest: MutationIdentity | null;
  activate(port: ProjectRunPort, ownerUserId: string, projectId: string): Promise<void>;
  refresh(port: ProjectRunPort): Promise<void>;
  monitor(port: ProjectRunPort): Promise<void>;
  analyze(port: ProjectRunPort): Promise<void>;
  selectTarget(port: ProjectRunPort, targetId: string): Promise<void>;
  setToolchain(kind: string, toolchainId: string): void;
  addEnvironmentVariable(): void;
  removeEnvironmentVariable(id: string): void;
  setEnvironmentVariableKey(id: string, key: string): void;
  setEnvironmentVariableValue(id: string, value: string): void;
  saveEnvironment(port: ProjectRunPort): Promise<void>;
  start(port: ProjectRunPort): Promise<void>;
  stop(port: ProjectRunPort, instanceId: string): Promise<void>;
  deleteInstance(port: ProjectRunPort, instanceId: string): Promise<void>;
  setFavorite(port: ProjectRunPort, isFavorite: boolean): Promise<void>;
  removeProject(port: ProjectRunPort): Promise<boolean>;
  dismissError(): void;
  dismissNotice(): void;
  deactivate(): void;
  reset(): void;
}

type GetState = () => ProjectRunStateStore;
type SetState = (
  value: Partial<ProjectRunStateStore>
    | ((state: ProjectRunStateStore) => Partial<ProjectRunStateStore>),
) => void;

export const useProjectRunStore = create<ProjectRunStateStore>((set, get) => ({
  ...emptyState(),
  generation: 0,
  requestSequence: 0,
  draftSequence: 0,

  async activate(port, ownerInput, projectInput) {
    const ownerUserId = ownerInput.trim();
    const projectId = projectInput.trim();
    if (!ownerUserId || !projectId) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId === ownerUserId && current.projectId === projectId) {
      if (!current.hasLoaded && !current.isLoading) await get().refresh(port);
      return;
    }
    set({
      ...emptyState(),
      ownerUserId,
      projectId,
      generation: current.generation + 1,
      requestSequence: current.requestSequence + 1,
      draftSequence: current.draftSequence + 1,
    });
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.projectId || current.activeMutationRequest) return;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeLoadRequest: requestId,
      isLoading: !current.hasLoaded,
      errorMessage: null,
      monitorWarning: null,
      notice: null,
    });
    try {
      const [analysis, runState, favorites] = await Promise.all([
        port.analyze({ ownerUserId, projectId }),
        port.loadState({ ownerUserId, projectId }),
        port.loadFavorites({ ownerUserId }),
      ]);
      if (!acceptsLoad(get(), ownerUserId, projectId, generation, requestId)) return;
      validateProjectResponse(projectId, analysis, runState);
      set((state) => ({
        catalog: analysis.catalog,
        environment: analysis.environment,
        runState,
        selectedToolchains: { ...analysis.environment.selectedToolchains },
        environmentVariables: environmentDrafts(analysis.environment),
        isFavorite: favorites.projectIds.includes(projectId),
        isLoading: false,
        hasLoaded: true,
        activeLoadRequest: null,
        draftSequence: state.draftSequence + 1,
      }));
    } catch (error) {
      if (!acceptsLoad(get(), ownerUserId, projectId, generation, requestId)) return;
      set({
        isLoading: false,
        hasLoaded: true,
        activeLoadRequest: null,
        errorMessage: errorMessage(error, "项目设置暂时无法读取。"),
      });
    }
  },

  async monitor(port) {
    const initial = get();
    if (!initial.ownerUserId || !initial.projectId || initial.activeMonitorRequest != null) return;
    const ownerUserId = initial.ownerUserId;
    const projectId = initial.projectId;
    const generation = initial.generation;
    const monitorId = initial.requestSequence + 1;
    set({ requestSequence: monitorId, activeMonitorRequest: monitorId });

    while (acceptsMonitor(get(), ownerUserId, projectId, generation, monitorId)) {
      const afterRevision = get().runState?.revision ?? 0;
      try {
        const change = await port.pollChanges({
          ownerUserId,
          projectId,
          afterRevision,
          timeoutMs: 30_000,
        });
        if (!acceptsMonitor(get(), ownerUserId, projectId, generation, monitorId)) return;
        if (change.state.projectId !== projectId || change.state.revision < afterRevision) {
          throw new Error("运行监控返回了不匹配的项目状态。");
        }
        if (!get().activeMutationRequest && change.state.revision >= (get().runState?.revision ?? 0)) {
          set({ runState: change.state, monitorWarning: null });
        }
      } catch (error) {
        if (!acceptsMonitor(get(), ownerUserId, projectId, generation, monitorId)) return;
        set({
          activeMonitorRequest: null,
          monitorWarning: errorMessage(error, "实时运行状态暂时无法刷新。"),
        });
        return;
      }
    }
  },

  async analyze(port) {
    await mutateAnalysis(port, "analyze", async (scope) => port.analyze(scope), "analyzed", get, set);
  },

  async selectTarget(port, targetId) {
    const target = get().catalog?.targets.find((candidate) => candidate.id === targetId);
    if (!target || target.id === get().catalog?.defaultTargetId) return;
    await mutateAnalysis(
      port,
      "defaultTarget",
      async (scope) => port.setDefaultTarget({ ...scope, targetId: target.id }),
      "defaultTargetSaved",
      get,
      set,
    );
  },

  setToolchain(kind, toolchainId) {
    const current = get();
    if (!current.environment || current.activeMutationRequest || !current.environment.toolchainOptions[kind]) return;
    const selectedToolchains = { ...current.selectedToolchains };
    if (toolchainId) selectedToolchains[kind] = toolchainId;
    else delete selectedToolchains[kind];
    set({ selectedToolchains, draftSequence: current.draftSequence + 1 });
  },

  addEnvironmentVariable() {
    const current = get();
    if (current.environmentVariables.length >= 128) return;
    set({
      environmentVariables: [...current.environmentVariables, {
        id: `draft-${current.draftSequence + 1}`,
        key: "",
        value: "",
        hasStoredValue: false,
        valueDirty: false,
      }],
      draftSequence: current.draftSequence + 1,
    });
  },

  removeEnvironmentVariable(id) {
    const current = get();
    set({
      environmentVariables: current.environmentVariables.filter((variable) => variable.id !== id),
      draftSequence: current.draftSequence + 1,
    });
  },

  setEnvironmentVariableKey(id, key) {
    updateEnvironmentDraft(id, (draft) => ({ ...draft, key }), get, set);
  },

  setEnvironmentVariableValue(id, value) {
    updateEnvironmentDraft(id, (draft) => ({ ...draft, value, valueDirty: true }), get, set);
  },

  async saveEnvironment(port) {
    const current = get();
    if (!current.ownerUserId || !current.projectId || current.activeMutationRequest) return;
    const validationError = validateEnvironmentDrafts(current.environmentVariables);
    if (validationError) {
      set({ errorMessage: validationError, notice: null });
      return;
    }
    const frozenDraftSequence = current.draftSequence;
    const selectedToolchains = Object.fromEntries(
      Object.entries(current.selectedToolchains).filter(([, value]) => value.length > 0),
    );
    const environmentVariables = current.environmentVariables.map((variable) => ({
      key: variable.key.trim(),
      value: variable.valueDirty ? variable.value : null,
    }));
    await mutateAnalysis(
      port,
      "environment",
      async ({ ownerUserId, projectId }) => port.updateEnvironment({
        ownerUserId,
        projectId,
        selectedToolchains,
        environmentVariables,
      }),
      "environmentSaved",
      get,
      set,
      frozenDraftSequence,
    );
  },

  async start(port) {
    const target = selectedProjectRunTarget(get().catalog);
    if (!target || (get().environment?.validationIssues.length ?? 0) > 0) return;
    await mutateRunState(
      port,
      "start",
      async (scope) => port.start({ ...scope, targetId: target.id }),
      "instanceStarted",
      get,
      set,
    );
  },

  async stop(port, instanceId) {
    if (!get().runState?.instances.some((instance) => instance.id === instanceId && instance.isRunning)) return;
    await mutateRunState(
      port,
      "stop",
      async (scope) => port.stop({ ...scope, instanceId }),
      "stopRequested",
      get,
      set,
    );
  },

  async deleteInstance(port, instanceId) {
    if (!get().runState?.instances.some((instance) => instance.id === instanceId)) return;
    await mutateRunState(
      port,
      "deleteInstance",
      async (scope) => port.deleteInstance({ ...scope, instanceId }),
      "instanceDeleted",
      get,
      set,
    );
  },

  async setFavorite(port, isFavorite) {
    const request = beginMutation("favorite", get, set);
    if (!request) return;
    try {
      const snapshot = await port.setFavorite({
        ownerUserId: request.ownerUserId,
        projectId: request.projectId,
        isFavorite,
      });
      if (!acceptsMutation(get(), request)) return;
      if (snapshot.projectIds.includes(request.projectId) !== isFavorite) {
        throw new Error("宠物快捷访问返回了不匹配的项目状态。");
      }
      finishMutation(set, { isFavorite, notice: "favoriteSaved" });
    } catch (error) {
      if (!acceptsMutation(get(), request)) return;
      const message = errorMessage(error, "宠物快捷访问偏好保存失败。");
      try {
        const snapshot = await port.loadFavorites({ ownerUserId: request.ownerUserId });
        if (!acceptsMutation(get(), request)) return;
        finishMutation(set, { isFavorite: snapshot.projectIds.includes(request.projectId), errorMessage: message });
      } catch {
        if (acceptsMutation(get(), request)) finishMutation(set, { errorMessage: message });
      }
    }
  },

  async removeProject(port) {
    const request = beginMutation("removeProject", get, set);
    if (!request) return false;
    try {
      const receipt = await port.removeProject({ ownerUserId: request.ownerUserId, projectId: request.projectId });
      if (!acceptsMutation(get(), request)) return false;
      if (receipt.projectId !== request.projectId || !receipt.directoryPreserved) {
        throw new Error("项目移除返回了不匹配的结果。");
      }
      finishMutation(set);
      return true;
    } catch (error) {
      if (!acceptsMutation(get(), request)) return false;
      finishMutation(set, { errorMessage: errorMessage(error, "无法从 ChatOS 移除项目。") });
      return false;
    }
  },

  dismissError() {
    set({ errorMessage: null, monitorWarning: null });
  },

  dismissNotice() {
    set({ notice: null });
  },

  deactivate() {
    const current = get();
    set({
      ...emptyState(),
      generation: current.generation + 1,
      requestSequence: current.requestSequence + 1,
      draftSequence: current.draftSequence + 1,
    });
  },

  reset() {
    get().deactivate();
  },
}));

interface FrozenRequest {
  id: number;
  kind: ProjectRunMutationKind;
  ownerUserId: string;
  projectId: string;
  generation: number;
}

function beginMutation(
  kind: ProjectRunMutationKind,
  get: GetState,
  set: SetState,
): FrozenRequest | null {
  const current = get();
  if (!current.ownerUserId || !current.projectId || current.activeMutationRequest) return null;
  const id = current.requestSequence + 1;
  set({
    requestSequence: id,
    activeLoadRequest: null,
    activeMutationRequest: { id, kind },
    isLoading: false,
    isMutating: true,
    mutationKind: kind,
    errorMessage: null,
    monitorWarning: null,
    notice: null,
  });
  return {
    id,
    kind,
    ownerUserId: current.ownerUserId,
    projectId: current.projectId,
    generation: current.generation,
  };
}

async function mutateAnalysis(
  port: ProjectRunPort,
  kind: ProjectRunMutationKind,
  operation: (scope: { ownerUserId: string; projectId: string }) => Promise<ProjectRunAnalysisSnapshot>,
  notice: ProjectRunNotice,
  get: GetState,
  set: SetState,
  frozenDraftSequence?: number,
) {
  const request = beginMutation(kind, get, set);
  if (!request) return;
  try {
    const analysis = await operation({ ownerUserId: request.ownerUserId, projectId: request.projectId });
    if (!acceptsMutation(get(), request)) return;
    validateAnalysis(request.projectId, analysis);
    applyAnalysis(set, get, analysis, notice, frozenDraftSequence);
  } catch (error) {
    if (!acceptsMutation(get(), request)) return;
    const message = errorMessage(error, "项目运行设置操作失败。");
    try {
      const analysis = await port.analyze({ ownerUserId: request.ownerUserId, projectId: request.projectId });
      if (!acceptsMutation(get(), request)) return;
      validateAnalysis(request.projectId, analysis);
      applyAnalysis(set, get, analysis, null, frozenDraftSequence, message);
    } catch {
      if (acceptsMutation(get(), request)) finishMutation(set, { errorMessage: message });
    }
  }
}

async function mutateRunState(
  port: ProjectRunPort,
  kind: ProjectRunMutationKind,
  operation: (scope: { ownerUserId: string; projectId: string }) => Promise<ProjectRunState>,
  notice: ProjectRunNotice,
  get: GetState,
  set: SetState,
) {
  const request = beginMutation(kind, get, set);
  if (!request) return;
  try {
    const runState = await operation({ ownerUserId: request.ownerUserId, projectId: request.projectId });
    if (!acceptsMutation(get(), request)) return;
    validateRunState(request.projectId, runState);
    finishMutation(set, { runState, notice });
  } catch (error) {
    if (!acceptsMutation(get(), request)) return;
    const message = errorMessage(error, "项目运行操作失败。");
    try {
      const runState = await port.loadState({ ownerUserId: request.ownerUserId, projectId: request.projectId });
      if (!acceptsMutation(get(), request)) return;
      validateRunState(request.projectId, runState);
      finishMutation(set, { runState, errorMessage: message });
    } catch {
      if (acceptsMutation(get(), request)) finishMutation(set, { errorMessage: message });
    }
  }
}

function applyAnalysis(
  set: SetState,
  get: GetState,
  analysis: ProjectRunAnalysisSnapshot,
  notice: ProjectRunNotice | null,
  frozenDraftSequence?: number,
  errorMessageValue: string | null = null,
) {
  const preserveDrafts = frozenDraftSequence != null && get().draftSequence !== frozenDraftSequence;
  finishMutation(set, {
    catalog: analysis.catalog,
    environment: analysis.environment,
    ...(preserveDrafts ? {} : {
      selectedToolchains: { ...analysis.environment.selectedToolchains },
      environmentVariables: environmentDrafts(analysis.environment),
    }),
    notice,
    errorMessage: errorMessageValue,
  });
}

function finishMutation(set: SetState, extra: Partial<ProjectRunStateStore> = {}) {
  set({
    ...extra,
    activeMutationRequest: null,
    isMutating: false,
    mutationKind: null,
  });
}

function updateEnvironmentDraft(
  id: string,
  update: (draft: ProjectRunEnvironmentDraft) => ProjectRunEnvironmentDraft,
  get: GetState,
  set: SetState,
) {
  const current = get();
  const index = current.environmentVariables.findIndex((variable) => variable.id === id);
  if (index < 0) return;
  const environmentVariables = [...current.environmentVariables];
  environmentVariables[index] = update(environmentVariables[index]);
  set({ environmentVariables, draftSequence: current.draftSequence + 1 });
}

function validateEnvironmentDrafts(drafts: ProjectRunEnvironmentDraft[]) {
  const keys = new Set<string>();
  for (const draft of drafts) {
    const key = draft.key.trim();
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(key) || key.length > 128) {
      return "环境变量名只能包含字母、数字和下划线，且不能以数字开头。";
    }
    if (keys.has(key)) return `环境变量 ${key} 重复。`;
    keys.add(key);
    if (!draft.hasStoredValue && !draft.valueDirty) return `请为环境变量 ${key} 输入安全值。`;
    if (new TextEncoder().encode(draft.value).byteLength > 64 * 1_024) {
      return `环境变量 ${key} 的值超过安全上限。`;
    }
  }
  return null;
}

function validateProjectResponse(
  projectId: string,
  analysis: ProjectRunAnalysisSnapshot,
  runState: ProjectRunState,
) {
  validateAnalysis(projectId, analysis);
  validateRunState(projectId, runState);
}

function validateAnalysis(projectId: string, analysis: ProjectRunAnalysisSnapshot) {
  if (analysis.catalog.projectId !== projectId) throw new Error("项目分析返回了其他项目的数据。");
}

function validateRunState(projectId: string, runState: ProjectRunState) {
  if (runState.projectId !== projectId) throw new Error("项目运行返回了其他项目的数据。");
}

function acceptsLoad(
  state: ProjectRunStateStore,
  ownerUserId: string,
  projectId: string,
  generation: number,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.projectId === projectId
    && state.generation === generation
    && state.activeLoadRequest === requestId
    && state.activeMutationRequest == null;
}

function acceptsMonitor(
  state: ProjectRunStateStore,
  ownerUserId: string,
  projectId: string,
  generation: number,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.projectId === projectId
    && state.generation === generation
    && state.activeMonitorRequest === requestId;
}

function acceptsMutation(state: ProjectRunStateStore, request: FrozenRequest) {
  return state.ownerUserId === request.ownerUserId
    && state.projectId === request.projectId
    && state.generation === request.generation
    && state.activeMutationRequest?.id === request.id
    && state.activeMutationRequest.kind === request.kind;
}

function emptyState() {
  return {
    ownerUserId: null,
    projectId: null,
    catalog: null,
    environment: null,
    runState: null,
    selectedToolchains: {},
    environmentVariables: [],
    isFavorite: false,
    isLoading: false,
    hasLoaded: false,
    isMutating: false,
    mutationKind: null,
    errorMessage: null,
    monitorWarning: null,
    notice: null,
    activeLoadRequest: null,
    activeMonitorRequest: null,
    activeMutationRequest: null,
  } satisfies Omit<ProjectRunStateStore,
    "generation" | "requestSequence" | "draftSequence"
    | "activate" | "refresh" | "monitor" | "analyze" | "selectTarget"
    | "setToolchain" | "addEnvironmentVariable" | "removeEnvironmentVariable"
    | "setEnvironmentVariableKey" | "setEnvironmentVariableValue" | "saveEnvironment"
    | "start" | "stop" | "deleteInstance" | "setFavorite" | "removeProject"
    | "dismissError" | "dismissNotice" | "deactivate" | "reset">;
}

function errorMessage(error: unknown, fallback: string) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return fallback;
}
