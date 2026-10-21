import { create } from "zustand";
import type {
  AgentSkillDefinition,
  AgentSkillKind,
  AgentSkillMutationInput,
} from "../domain/agentSkillManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentSkillManagementState {
  ownerUserId: string | null;
  items: AgentSkillDefinition[];
  isLoading: boolean;
  isSaving: boolean;
  error: string | null;
  requestSequence: number;
  activeRequestId: number;
  mutationRequestId: number;
  activate(port: AgentManagementPort, ownerUserId: string): Promise<void>;
  save(port: AgentManagementPort, input: AgentSkillMutationInput): Promise<AgentSkillDefinition | null>;
  resetOverride(port: AgentManagementPort, kind: AgentSkillKind, key: string): Promise<AgentSkillDefinition | null>;
  clearError(): void;
  reset(): void;
}

export const useAgentSkillManagementStore = create<AgentSkillManagementState>((set, get) => ({
  ownerUserId: null,
  items: [],
  isLoading: false,
  isSaving: false,
  error: null,
  requestSequence: 0,
  activeRequestId: 0,
  mutationRequestId: 0,

  async activate(port, ownerInput) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId || !port.loadSkillCatalog) {
      get().reset();
      return;
    }
    const requestId = get().requestSequence + 1;
    set({
      ownerUserId,
      items: [],
      isLoading: true,
      isSaving: false,
      error: null,
      requestSequence: requestId,
      activeRequestId: requestId,
      mutationRequestId: 0,
    });
    try {
      const items = await port.loadSkillCatalog(ownerUserId);
      if (!accepts(get(), ownerUserId, requestId, "activeRequestId")) return;
      set({ items, isLoading: false, activeRequestId: 0 });
    } catch (error) {
      if (!accepts(get(), ownerUserId, requestId, "activeRequestId")) return;
      set({ isLoading: false, error: errorMessage(error), activeRequestId: 0 });
    }
  },

  async save(port, input) {
    const current = get();
    if (!current.ownerUserId || current.isSaving || !port.saveSkillOverride) return null;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({ isSaving: true, error: null, requestSequence: requestId, mutationRequestId: requestId });
    try {
      const saved = await port.saveSkillOverride(ownerUserId, input);
      if (!accepts(get(), ownerUserId, requestId, "mutationRequestId")) return null;
      set((state) => ({
        items: replaceItem(state.items, saved),
        isSaving: false,
        mutationRequestId: 0,
      }));
      return saved;
    } catch (error) {
      if (!accepts(get(), ownerUserId, requestId, "mutationRequestId")) return null;
      set({ isSaving: false, error: errorMessage(error), mutationRequestId: 0 });
      return null;
    }
  },

  async resetOverride(port, kind, key) {
    const current = get();
    if (!current.ownerUserId || current.isSaving || !port.resetSkillOverride) return null;
    const ownerUserId = current.ownerUserId;
    const requestId = current.requestSequence + 1;
    set({ isSaving: true, error: null, requestSequence: requestId, mutationRequestId: requestId });
    try {
      const restored = await port.resetSkillOverride(ownerUserId, kind, key);
      if (!accepts(get(), ownerUserId, requestId, "mutationRequestId")) return null;
      set((state) => ({
        items: replaceItem(state.items, restored),
        isSaving: false,
        mutationRequestId: 0,
      }));
      return restored;
    } catch (error) {
      if (!accepts(get(), ownerUserId, requestId, "mutationRequestId")) return null;
      set({ isSaving: false, error: errorMessage(error), mutationRequestId: 0 });
      return null;
    }
  },

  clearError() {
    set({ error: null });
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      items: [],
      isLoading: false,
      isSaving: false,
      error: null,
      requestSequence: state.requestSequence + 1,
      activeRequestId: 0,
      mutationRequestId: 0,
    }));
  },
}));

function replaceItem(items: AgentSkillDefinition[], replacement: AgentSkillDefinition) {
  return items.map((item) => (
    item.kind === replacement.kind && item.key === replacement.key ? replacement : item
  ));
}

function accepts(
  state: AgentSkillManagementState,
  ownerUserId: string,
  requestId: number,
  field: "activeRequestId" | "mutationRequestId",
) {
  return state.ownerUserId === ownerUserId && state[field] === requestId;
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Skill 管理暂时不可用。";
}
