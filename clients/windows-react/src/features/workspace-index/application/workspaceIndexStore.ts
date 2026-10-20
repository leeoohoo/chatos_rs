import { create } from "zustand";
import {
  resolveWorkspaceIndex,
  type LocalProject,
  type WorkspaceIndexSnapshot,
  type WorkspaceRelations,
} from "../domain/workspaceIndex";
import type { WorkspaceIndexPort } from "../ports/WorkspaceIndexPort";

const emptyRelations: WorkspaceRelations = { contacts: [], conversations: [] };
const emptySnapshot: WorkspaceIndexSnapshot = { contacts: [], projects: [] };

interface WorkspaceIndexState {
  ownerUserId: string | null;
  requestId: number;
  localProjects: LocalProject[];
  relations: WorkspaceRelations;
  snapshot: WorkspaceIndexSnapshot;
  isLoading: boolean;
  errorMessage: string | null;
  load(
    port: WorkspaceIndexPort,
    ownerUserId: string,
    hostReady: boolean,
  ): Promise<void>;
  acceptCreatedProject(ownerUserId: string, project: LocalProject): void;
  acceptRemovedProject(ownerUserId: string, projectId: string): void;
  reset(): void;
}

export const useWorkspaceIndexStore = create<WorkspaceIndexState>((set, get) => ({
  ownerUserId: null,
  requestId: 0,
  localProjects: [],
  relations: emptyRelations,
  snapshot: emptySnapshot,
  isLoading: false,
  errorMessage: null,

  async load(port, ownerUserId, hostReady) {
    const owner = ownerUserId.trim();
    const requestId = get().requestId + 1;
    if (!owner) {
      get().reset();
      return;
    }
    const ownerChanged = get().ownerUserId !== owner;
    const preservedRelations = ownerChanged ? emptyRelations : get().relations;
    set({
      ownerUserId: owner,
      requestId,
      relations: preservedRelations,
      snapshot: ownerChanged ? emptySnapshot : get().snapshot,
      isLoading: true,
      errorMessage: null,
    });
    let localProjects: LocalProject[];
    try {
      localProjects = await port.listLocalProjects(owner);
    } catch (error) {
      if (!accepts(get, requestId, owner)) return;
      set({ isLoading: false, errorMessage: errorMessage(error) });
      return;
    }
    if (!accepts(get, requestId, owner)) return;
    set({
      localProjects,
      snapshot: resolveWorkspaceIndex(localProjects, preservedRelations),
    });
    if (!hostReady) {
      set({ isLoading: false });
      return;
    }
    try {
      const relations = await port.refreshRelations(owner);
      if (!accepts(get, requestId, owner)) return;
      set({
        relations,
        snapshot: resolveWorkspaceIndex(localProjects, relations),
        isLoading: false,
      });
    } catch (error) {
      if (!accepts(get, requestId, owner)) return;
      set({
        isLoading: false,
        errorMessage: errorMessage(error),
        snapshot: resolveWorkspaceIndex(localProjects, preservedRelations),
      });
    }
  },

  acceptCreatedProject(ownerInput, project) {
    const ownerUserId = ownerInput.trim();
    const current = get();
    if (!ownerUserId || current.ownerUserId !== ownerUserId) return;
    const localProjects = [
      ...current.localProjects.filter((candidate) => candidate.id !== project.id),
      project,
    ];
    set({
      localProjects,
      snapshot: resolveWorkspaceIndex(localProjects, current.relations),
      errorMessage: null,
    });
  },

  acceptRemovedProject(ownerInput, projectInput) {
    const ownerUserId = ownerInput.trim();
    const projectId = projectInput.trim();
    const current = get();
    if (!ownerUserId || !projectId || current.ownerUserId !== ownerUserId) return;
    const localProjects = current.localProjects.filter((project) => project.id !== projectId);
    set({
      localProjects,
      snapshot: resolveWorkspaceIndex(localProjects, current.relations),
      errorMessage: null,
    });
  },

  reset() {
    set({
      ownerUserId: null,
      requestId: get().requestId + 1,
      localProjects: [],
      relations: emptyRelations,
      snapshot: emptySnapshot,
      isLoading: false,
      errorMessage: null,
    });
  },
}));

function accepts(
  get: () => WorkspaceIndexState,
  requestId: number,
  ownerUserId: string,
): boolean {
  return get().requestId === requestId && get().ownerUserId === ownerUserId;
}

function errorMessage(error: unknown): string {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "资源索引加载失败。";
}

