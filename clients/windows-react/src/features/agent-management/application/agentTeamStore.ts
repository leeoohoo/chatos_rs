import { create } from "zustand";
import type { AgentTeamSummary, CreateAgentTeamInput } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentTeamState {
  ownerUserId: string | null;
  teams: AgentTeamSummary[];
  isLoading: boolean;
  isCreating: boolean;
  error: string | null;
  sequence: number;
  listRequestId: number;
  mutationRequestId: number;
  activate(port: AgentManagementPort, ownerUserId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  createTeam(port: AgentManagementPort, input: CreateAgentTeamInput): Promise<AgentTeamSummary | null>;
  reset(): void;
}

export const useAgentTeamStore = create<AgentTeamState>((set, get) => ({
  ownerUserId: null,
  teams: [],
  isLoading: false,
  isCreating: false,
  error: null,
  sequence: 0,
  listRequestId: 0,
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
        teams: [],
        isLoading: false,
        isCreating: false,
        error: null,
        sequence: current.sequence + 1,
        listRequestId: 0,
        mutationRequestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !port.listTeams) return;
    const ownerUserId = current.ownerUserId;
    const requestId = current.sequence + 1;
    set({
      sequence: requestId,
      listRequestId: requestId,
      isLoading: current.teams.length === 0,
      error: null,
    });
    try {
      const teams = await port.listTeams(ownerUserId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.listRequestId !== requestId) return;
      set({ teams, isLoading: false, error: null, listRequestId: 0 });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.listRequestId !== requestId) return;
      set({ isLoading: false, error: errorMessage(failure), listRequestId: 0 });
    }
  },

  async createTeam(port, input) {
    const current = get();
    if (!current.ownerUserId || !port.createTeam || current.isCreating) return null;
    const ownerUserId = current.ownerUserId;
    const requestId = current.sequence + 1;
    set({
      sequence: requestId,
      listRequestId: 0,
      mutationRequestId: requestId,
      isLoading: false,
      isCreating: true,
      error: null,
    });
    try {
      const team = await port.createTeam(ownerUserId, input);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.mutationRequestId !== requestId) return null;
      set({
        teams: [team, ...state.teams.filter((candidate) => candidate.id !== team.id)],
        isCreating: false,
        mutationRequestId: 0,
      });
      return team;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.mutationRequestId !== requestId) return null;
      set({ isCreating: false, error: errorMessage(failure), mutationRequestId: 0 });
      return null;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      teams: [],
      isLoading: false,
      isCreating: false,
      error: null,
      sequence: state.sequence + 1,
      listRequestId: 0,
      mutationRequestId: 0,
    }));
  },
}));

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "Agent 团队暂时无法读取。";
}
