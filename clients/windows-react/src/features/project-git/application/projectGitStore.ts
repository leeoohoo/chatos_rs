import { create } from "zustand";
import type {
  ProjectGitChange,
  ProjectGitDiff,
  ProjectGitSnapshot,
} from "../domain/projectGit";
import {
  stagedProjectGitChanges,
  unavailableProjectGitSnapshot,
  workingTreeProjectGitChanges,
} from "../domain/projectGit";
import type { ProjectGitPort } from "../ports/ProjectGitPort";

export type ProjectGitMutationKind =
  | "initialize"
  | "stage"
  | "unstage"
  | "commit"
  | "switchBranch"
  | "createBranch"
  | "mergeBranch"
  | "saveRemote"
  | "removeRemote"
  | "pull"
  | "push";

type ProjectGitMutation =
  | { kind: "initialize" }
  | { kind: "stage"; paths: string[] }
  | { kind: "unstage"; paths: string[] }
  | { kind: "commit"; message: string; messageRevision: number }
  | { kind: "switchBranch"; branch: string }
  | { kind: "createBranch"; name: string; switchToBranch: boolean }
  | { kind: "mergeBranch"; branch: string }
  | { kind: "saveRemote"; originalName: string | null; name: string; url: string | null }
  | { kind: "removeRemote"; name: string }
  | { kind: "pull" }
  | { kind: "push" };

interface DiffRequestIdentity {
  id: number;
  path: string;
  staged: boolean;
}

interface MutationRequestIdentity {
  id: number;
  kind: ProjectGitMutationKind;
}

export interface ProjectGitState {
  ownerUserId: string | null;
  projectId: string | null;
  snapshot: ProjectGitSnapshot;
  selectedDiff: ProjectGitDiff | null;
  isLoading: boolean;
  isDiffLoading: boolean;
  isPerformingOperation: boolean;
  mutationKind: ProjectGitMutationKind | null;
  errorMessage: string | null;
  worktreeRevision: number;
  commitMessage: string;
  commitMessageRevision: number;
  generation: number;
  requestSequence: number;
  activeLoadRequest: number | null;
  activeDiffRequest: DiffRequestIdentity | null;
  activeMutationRequest: MutationRequestIdentity | null;
  hasLoaded: boolean;
  activate(port: ProjectGitPort, ownerUserId: string, projectId: string): Promise<void>;
  load(port: ProjectGitPort): Promise<void>;
  showDiff(port: ProjectGitPort, change: ProjectGitChange, staged: boolean): Promise<void>;
  clearDiff(): void;
  setCommitMessage(value: string): void;
  initializeRepository(port: ProjectGitPort): Promise<void>;
  stage(port: ProjectGitPort, change: ProjectGitChange): Promise<void>;
  stageAll(port: ProjectGitPort): Promise<void>;
  unstage(port: ProjectGitPort, change: ProjectGitChange): Promise<void>;
  unstageAll(port: ProjectGitPort): Promise<void>;
  commit(port: ProjectGitPort): Promise<void>;
  switchBranch(port: ProjectGitPort, branch: string): Promise<void>;
  createBranch(port: ProjectGitPort, name: string, switchToBranch: boolean): Promise<void>;
  mergeBranch(port: ProjectGitPort, branch: string): Promise<void>;
  saveRemote(port: ProjectGitPort, originalName: string | null, name: string, url: string | null): Promise<void>;
  removeRemote(port: ProjectGitPort, name: string): Promise<void>;
  pull(port: ProjectGitPort): Promise<void>;
  push(port: ProjectGitPort): Promise<void>;
  dismissError(): void;
  reset(): void;
}

type GetState = () => ProjectGitState;
type SetState = (partial: Partial<ProjectGitState>) => void;

export const useProjectGitStore = create<ProjectGitState>((set, get) => ({
  ownerUserId: null,
  projectId: null,
  snapshot: unavailableProjectGitSnapshot(""),
  selectedDiff: null,
  isLoading: false,
  isDiffLoading: false,
  isPerformingOperation: false,
  mutationKind: null,
  errorMessage: null,
  worktreeRevision: 0,
  commitMessage: "",
  commitMessageRevision: 0,
  generation: 0,
  requestSequence: 0,
  activeLoadRequest: null,
  activeDiffRequest: null,
  activeMutationRequest: null,
  hasLoaded: false,

  async activate(port, ownerInput, projectInput) {
    const ownerUserId = ownerInput.trim();
    const projectId = projectInput.trim();
    if (!ownerUserId || !projectId) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId === ownerUserId && current.projectId === projectId) {
      if (!current.hasLoaded && !current.isLoading) await get().load(port);
      return;
    }
    set({
      ...resetPresentation(current),
      ownerUserId,
      projectId,
      snapshot: unavailableProjectGitSnapshot(projectId),
      generation: current.generation + 1,
    });
    await get().load(port);
  },

  async load(port) {
    const current = get();
    if (!current.ownerUserId || !current.projectId || current.activeMutationRequest) return;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeLoadRequest: requestId,
      activeDiffRequest: null,
      isLoading: true,
      isDiffLoading: false,
      errorMessage: null,
    });
    try {
      const snapshot = await port.loadSnapshot({ ownerUserId, projectId });
      if (!acceptsLoad(get(), ownerUserId, projectId, generation, requestId)) return;
      if (snapshot.projectId !== projectId) throw new Error("Git 状态返回了其他项目的数据。");
      set({ snapshot, isLoading: false, activeLoadRequest: null, hasLoaded: true });
    } catch (error) {
      if (!acceptsLoad(get(), ownerUserId, projectId, generation, requestId)) return;
      set({
        isLoading: false,
        activeLoadRequest: null,
        hasLoaded: true,
        errorMessage: errorMessage(error),
      });
    }
  },

  async showDiff(port, change, staged) {
    const current = get();
    if (!current.ownerUserId || !current.projectId || current.activeMutationRequest) return;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const generation = current.generation;
    const request = { id: current.requestSequence + 1, path: change.path, staged };
    set({
      requestSequence: request.id,
      activeDiffRequest: request,
      isDiffLoading: true,
      errorMessage: null,
    });
    try {
      const diff = await port.loadDiff({ ownerUserId, projectId, path: change.path, staged });
      if (!acceptsDiff(get(), ownerUserId, projectId, generation, request)) return;
      if (diff.projectId !== projectId || diff.path !== change.path || diff.isStaged !== staged) {
        throw new Error("Git 差异返回了不匹配的文件。");
      }
      set({ selectedDiff: diff, activeDiffRequest: null, isDiffLoading: false });
    } catch (error) {
      if (!acceptsDiff(get(), ownerUserId, projectId, generation, request)) return;
      set({ activeDiffRequest: null, isDiffLoading: false, errorMessage: errorMessage(error) });
    }
  },

  clearDiff() {
    set({ activeDiffRequest: null, selectedDiff: null, isDiffLoading: false });
  },

  setCommitMessage(value) {
    set((state) => ({ commitMessage: value, commitMessageRevision: state.commitMessageRevision + 1 }));
  },

  async initializeRepository(port) {
    await mutate(port, { kind: "initialize" }, get, set);
  },

  async stage(port, change) {
    await mutate(port, { kind: "stage", paths: [change.path] }, get, set);
  },

  async stageAll(port) {
    const paths = workingTreeProjectGitChanges(get().snapshot).map((change) => change.path);
    if (paths.length) await mutate(port, { kind: "stage", paths }, get, set);
  },

  async unstage(port, change) {
    await mutate(port, { kind: "unstage", paths: [change.path] }, get, set);
  },

  async unstageAll(port) {
    const paths = stagedProjectGitChanges(get().snapshot).map((change) => change.path);
    if (paths.length) await mutate(port, { kind: "unstage", paths }, get, set);
  },

  async commit(port) {
    const current = get();
    await mutate(port, {
      kind: "commit",
      message: current.commitMessage,
      messageRevision: current.commitMessageRevision,
    }, get, set);
  },

  async switchBranch(port, branch) {
    if (branch === get().snapshot.currentBranch) return;
    await mutate(port, { kind: "switchBranch", branch }, get, set);
  },

  async createBranch(port, name, switchToBranch) {
    await mutate(port, { kind: "createBranch", name, switchToBranch }, get, set);
  },

  async mergeBranch(port, branch) {
    await mutate(port, { kind: "mergeBranch", branch }, get, set);
  },

  async saveRemote(port, originalName, name, url) {
    await mutate(port, { kind: "saveRemote", originalName, name, url }, get, set);
  },

  async removeRemote(port, name) {
    await mutate(port, { kind: "removeRemote", name }, get, set);
  },

  async pull(port) {
    await mutate(port, { kind: "pull" }, get, set);
  },

  async push(port) {
    await mutate(port, { kind: "push" }, get, set);
  },

  dismissError() {
    set({ errorMessage: null });
  },

  reset() {
    set((state) => ({
      ...resetPresentation(state),
      snapshot: unavailableProjectGitSnapshot(""),
      generation: state.generation + 1,
    }));
  },
}));

async function mutate(
  port: ProjectGitPort,
  mutation: ProjectGitMutation,
  get: GetState,
  set: SetState,
) {
  const current = get();
  if (!current.ownerUserId || !current.projectId || current.activeMutationRequest) return;
  const ownerUserId = current.ownerUserId;
  const projectId = current.projectId;
  const generation = current.generation;
  const request = { id: current.requestSequence + 1, kind: mutation.kind };
  set({
    requestSequence: request.id,
    activeMutationRequest: request,
    activeLoadRequest: null,
    activeDiffRequest: null,
    isLoading: false,
    isDiffLoading: false,
    isPerformingOperation: true,
    mutationKind: mutation.kind,
    errorMessage: null,
  });
  try {
    const result = await executeMutation(port, { ownerUserId, projectId }, mutation);
    if (!acceptsMutation(get(), ownerUserId, projectId, generation, request)) return;
    if (result.projectId !== projectId
      || (result.snapshot != null && result.snapshot.projectId !== projectId)) {
      throw new Error("Git 操作返回了其他项目的数据。");
    }
    const update: Partial<ProjectGitState> = {
      selectedDiff: null,
      activeMutationRequest: null,
      isPerformingOperation: false,
      mutationKind: null,
      errorMessage: result.committedWithoutRefresh
        ? "Git 操作已完成，但最新状态暂时无法刷新。"
        : null,
    };
    if (result.snapshot) update.snapshot = result.snapshot;
    if (updatesWorkingTree(mutation)) update.worktreeRevision = get().worktreeRevision + 1;
    if (mutation.kind === "commit" && get().commitMessageRevision === mutation.messageRevision) {
      update.commitMessage = "";
      update.commitMessageRevision = mutation.messageRevision + 1;
    }
    set(update);
  } catch (error) {
    if (!acceptsMutation(get(), ownerUserId, projectId, generation, request)) return;
    const operationError = errorMessage(error);
    let recovered: ProjectGitSnapshot | null = null;
    try {
      const snapshot = await port.loadSnapshot({ ownerUserId, projectId });
      if (snapshot.projectId === projectId) recovered = snapshot;
    } catch {
      // Preserve the original mutation failure, matching the macOS recovery contract.
    }
    if (!acceptsMutation(get(), ownerUserId, projectId, generation, request)) return;
    set({
      ...(recovered ? { snapshot: recovered } : {}),
      activeMutationRequest: null,
      isPerformingOperation: false,
      mutationKind: null,
      errorMessage: operationError,
    });
  }
}

async function executeMutation(
  port: ProjectGitPort,
  scope: { ownerUserId: string; projectId: string },
  mutation: ProjectGitMutation,
) {
  switch (mutation.kind) {
    case "initialize": return port.initializeRepository(scope);
    case "stage": return port.stagePaths({ ...scope, paths: mutation.paths });
    case "unstage": return port.unstagePaths({ ...scope, paths: mutation.paths });
    case "commit": return port.commit({ ...scope, message: mutation.message });
    case "switchBranch": return port.switchBranch({ ...scope, branch: mutation.branch });
    case "createBranch": return port.createBranch({ ...scope, name: mutation.name, switchToBranch: mutation.switchToBranch });
    case "mergeBranch": return port.mergeBranch({ ...scope, branch: mutation.branch });
    case "saveRemote": return port.saveRemote({ ...scope, originalName: mutation.originalName, name: mutation.name, url: mutation.url });
    case "removeRemote": return port.removeRemote({ ...scope, name: mutation.name });
    case "pull": return port.pull(scope);
    case "push": return port.push(scope);
  }
}

function updatesWorkingTree(mutation: ProjectGitMutation) {
  return mutation.kind === "switchBranch"
    || mutation.kind === "mergeBranch"
    || mutation.kind === "pull"
    || mutation.kind === "createBranch" && mutation.switchToBranch;
}

function acceptsLoad(
  state: ProjectGitState,
  ownerUserId: string,
  projectId: string,
  generation: number,
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && state.projectId === projectId
    && state.generation === generation
    && state.activeLoadRequest === requestId;
}

function acceptsDiff(
  state: ProjectGitState,
  ownerUserId: string,
  projectId: string,
  generation: number,
  request: DiffRequestIdentity,
) {
  return state.ownerUserId === ownerUserId
    && state.projectId === projectId
    && state.generation === generation
    && state.activeDiffRequest?.id === request.id
    && state.activeDiffRequest?.path === request.path
    && state.activeDiffRequest?.staged === request.staged;
}

function acceptsMutation(
  state: ProjectGitState,
  ownerUserId: string,
  projectId: string,
  generation: number,
  request: MutationRequestIdentity,
) {
  return state.ownerUserId === ownerUserId
    && state.projectId === projectId
    && state.generation === generation
    && state.activeMutationRequest?.id === request.id
    && state.activeMutationRequest?.kind === request.kind;
}

function resetPresentation(state: ProjectGitState): Partial<ProjectGitState> {
  return {
    ownerUserId: null,
    projectId: null,
    selectedDiff: null,
    isLoading: false,
    isDiffLoading: false,
    isPerformingOperation: false,
    mutationKind: null,
    errorMessage: null,
    worktreeRevision: 0,
    commitMessage: "",
    commitMessageRevision: state.commitMessageRevision + 1,
    activeLoadRequest: null,
    activeDiffRequest: null,
    activeMutationRequest: null,
    hasLoaded: false,
  };
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "Git 操作暂时无法完成。";
}
