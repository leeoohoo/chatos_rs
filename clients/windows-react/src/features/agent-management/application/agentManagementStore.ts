import { create } from "zustand";
import type {
  AgentProfileEditorCatalog,
  AgentProfileEditorData,
  AgentProfileMutationInput,
  AgentProfileSummary,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentManagementState {
  ownerUserId: string | null;
  profiles: AgentProfileSummary[];
  editorCatalog: AgentProfileEditorCatalog | null;
  editorProfile: AgentProfileEditorData | null;
  isLoading: boolean;
  isCatalogLoading: boolean;
  isEditorLoading: boolean;
  isSaving: boolean;
  error: string | null;
  requestSequence: number;
  activeRequestId: number;
  catalogRequestId: number;
  editorRequestId: number;
  mutationRequestId: number;
  activate(port: AgentManagementPort, ownerUserId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  loadEditorCatalog(port: AgentManagementPort, reportErrors?: boolean): Promise<AgentProfileEditorCatalog | null>;
  loadProfileEditor(port: AgentManagementPort, agentId: string): Promise<AgentProfileEditorData | null>;
  saveProfile(port: AgentManagementPort, input: AgentProfileMutationInput): Promise<boolean>;
  reset(): void;
}

export const useAgentManagementStore = create<AgentManagementState>((set, get) => ({
  ownerUserId: null,
  profiles: [],
  editorCatalog: null,
  editorProfile: null,
  isLoading: false,
  isCatalogLoading: false,
  isEditorLoading: false,
  isSaving: false,
  error: null,
  requestSequence: 0,
  activeRequestId: 0,
  catalogRequestId: 0,
  editorRequestId: 0,
  mutationRequestId: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId) {
      set({
        ownerUserId,
        profiles: [],
        editorCatalog: null,
        editorProfile: null,
        isLoading: false,
        isCatalogLoading: false,
        isEditorLoading: false,
        isSaving: false,
        error: null,
        requestSequence: current.requestSequence + 1,
        activeRequestId: 0,
        catalogRequestId: 0,
        editorRequestId: 0,
        mutationRequestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId) return;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      activeRequestId: requestId,
      isLoading: current.profiles.length === 0,
      error: null,
    });
    try {
      const profiles = await port.listProfiles(ownerUserId);
      if (!accepts(get(), ownerUserId, requestId)) return;
      set({ profiles, isLoading: false, error: null, activeRequestId: 0 });
    } catch (error) {
      if (!accepts(get(), ownerUserId, requestId)) return;
      set({ isLoading: false, error: errorMessage(error), activeRequestId: 0 });
    }
  },

  async loadEditorCatalog(port, reportErrors = true) {
    const current = get();
    if (!current.ownerUserId) return null;
    if (current.editorCatalog) return current.editorCatalog;
    if (current.isCatalogLoading) return current.editorCatalog;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      catalogRequestId: requestId,
      isCatalogLoading: current.editorCatalog == null,
      error: reportErrors ? null : current.error,
    });
    try {
      const editorCatalog = await port.loadEditorCatalog(ownerUserId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.catalogRequestId !== requestId) return null;
      set({ editorCatalog, isCatalogLoading: false, catalogRequestId: 0 });
      return editorCatalog;
    } catch (error) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.catalogRequestId !== requestId) return null;
      set({
        isCatalogLoading: false,
        error: reportErrors ? errorMessage(error) : state.error,
        catalogRequestId: 0,
      });
      return null;
    }
  },

  async loadProfileEditor(port, agentId) {
    const current = get();
    if (!current.ownerUserId || !agentId.trim()) return null;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      editorRequestId: requestId,
      editorProfile: null,
      isEditorLoading: true,
      error: null,
    });
    try {
      const editorProfile = await port.loadProfileEditor(ownerUserId, agentId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.editorRequestId !== requestId) return null;
      set({ editorProfile, isEditorLoading: false, editorRequestId: 0 });
      return editorProfile;
    } catch (error) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.editorRequestId !== requestId) return null;
      set({ isEditorLoading: false, error: errorMessage(error), editorRequestId: 0 });
      return null;
    }
  },

  async saveProfile(port, input) {
    const current = get();
    if (!current.ownerUserId || current.isSaving) return false;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({
      requestSequence: requestId,
      mutationRequestId: requestId,
      isSaving: true,
      error: null,
    });
    try {
      const saved = await port.saveProfile(ownerUserId, input);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.mutationRequestId !== requestId) return false;
      set({
        profiles: upsertProfile(state.profiles, saved),
        isSaving: false,
        mutationRequestId: 0,
      });
      return true;
    } catch (error) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.mutationRequestId !== requestId) return false;
      set({ isSaving: false, error: errorMessage(error), mutationRequestId: 0 });
      return false;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      profiles: [],
      editorCatalog: null,
      editorProfile: null,
      isLoading: false,
      isCatalogLoading: false,
      isEditorLoading: false,
      isSaving: false,
      error: null,
      requestSequence: state.requestSequence + 1,
      activeRequestId: 0,
      catalogRequestId: 0,
      editorRequestId: 0,
      mutationRequestId: 0,
    }));
  },
}));

function accepts(state: AgentManagementState, ownerUserId: string, requestId: number) {
  return state.ownerUserId === ownerUserId && state.activeRequestId === requestId;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "Agent 列表暂时无法读取。";
}

function upsertProfile(profiles: AgentProfileSummary[], saved: AgentProfileSummary) {
  return [...profiles.filter((profile) => profile.id !== saved.id), saved]
    .sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id));
}
