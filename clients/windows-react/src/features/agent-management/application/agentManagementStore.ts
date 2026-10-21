import { create } from "zustand";
import type { AgentProfileSummary } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentManagementState {
  ownerUserId: string | null;
  profiles: AgentProfileSummary[];
  isLoading: boolean;
  error: string | null;
  requestSequence: number;
  activeRequestId: number;
  activate(port: AgentManagementPort, ownerUserId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  reset(): void;
}

export const useAgentManagementStore = create<AgentManagementState>((set, get) => ({
  ownerUserId: null,
  profiles: [],
  isLoading: false,
  error: null,
  requestSequence: 0,
  activeRequestId: 0,

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
        isLoading: false,
        error: null,
        requestSequence: current.requestSequence + 1,
        activeRequestId: 0,
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

  reset() {
    set((state) => ({
      ownerUserId: null,
      profiles: [],
      isLoading: false,
      error: null,
      requestSequence: state.requestSequence + 1,
      activeRequestId: 0,
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
