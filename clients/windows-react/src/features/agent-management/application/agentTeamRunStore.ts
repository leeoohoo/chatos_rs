import { create } from "zustand";
import type { AgentTeamRunHistorySnapshot } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentTeamRunState {
  ownerUserId: string | null;
  teamId: string | null;
  snapshot: AgentTeamRunHistorySnapshot | null;
  isLoading: boolean;
  error: string | null;
  sequence: number;
  requestId: number;
  activate(port: AgentManagementPort, ownerUserId: string, teamId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  reset(): void;
}

export const useAgentTeamRunStore = create<AgentTeamRunState>((set, get) => ({
  ownerUserId: null,
  teamId: null,
  snapshot: null,
  isLoading: false,
  error: null,
  sequence: 0,
  requestId: 0,

  async activate(port, ownerInput, teamInput) {
    const ownerUserId = ownerInput.trim();
    const teamId = teamInput.trim();
    if (!ownerUserId || !teamId || !port.loadTeamRuns) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId || current.teamId !== teamId) {
      set({
        ownerUserId,
        teamId,
        snapshot: null,
        isLoading: false,
        error: null,
        sequence: current.sequence + 1,
        requestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.requestId !== 0 || !port.loadTeamRuns) return;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const requestId = current.sequence + 1;
    set({ sequence: requestId, requestId, isLoading: current.snapshot == null, error: null });
    try {
      const snapshot = await port.loadTeamRuns(ownerUserId, teamId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      if (snapshot.teamId !== teamId) throw new Error("Agent 团队运行记录身份不一致，请刷新后重试。");
      set({ snapshot, isLoading: false, requestId: 0 });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      set({ isLoading: false, error: errorMessage(failure), requestId: 0 });
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      teamId: null,
      snapshot: null,
      isLoading: false,
      error: null,
      sequence: state.sequence + 1,
      requestId: 0,
    }));
  },
}));

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "Agent 团队运行记录暂时不可用。";
}
