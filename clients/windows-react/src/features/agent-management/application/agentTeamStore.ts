import { create } from "zustand";
import type { AgentTeamDetail, AgentTeamSummary, CreateAgentTeamInput } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentTeamState {
  ownerUserId: string | null;
  teams: AgentTeamSummary[];
  selectedTeamId: string | null;
  detail: AgentTeamDetail | null;
  isLoading: boolean;
  isCreating: boolean;
  isDetailLoading: boolean;
  isInviting: boolean;
  error: string | null;
  sequence: number;
  listRequestId: number;
  mutationRequestId: number;
  detailRequestId: number;
  inviteRequestId: number;
  activate(port: AgentManagementPort, ownerUserId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  createTeam(port: AgentManagementPort, input: CreateAgentTeamInput): Promise<AgentTeamSummary | null>;
  selectTeam(port: AgentManagementPort, teamId: string): Promise<void>;
  inviteAgent(port: AgentManagementPort, teamId: string, agentId: string): Promise<boolean>;
  reset(): void;
}

export const useAgentTeamStore = create<AgentTeamState>((set, get) => ({
  ownerUserId: null,
  teams: [],
  selectedTeamId: null,
  detail: null,
  isLoading: false,
  isCreating: false,
  isDetailLoading: false,
  isInviting: false,
  error: null,
  sequence: 0,
  listRequestId: 0,
  mutationRequestId: 0,
  detailRequestId: 0,
  inviteRequestId: 0,

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
        selectedTeamId: null,
        detail: null,
        isLoading: false,
        isCreating: false,
        isDetailLoading: false,
        isInviting: false,
        error: null,
        sequence: current.sequence + 1,
        listRequestId: 0,
        mutationRequestId: 0,
        detailRequestId: 0,
        inviteRequestId: 0,
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
      const selectedStillExists = state.selectedTeamId != null
        && teams.some((team) => team.id === state.selectedTeamId);
      const selectedSummary = teams.find((team) => team.id === state.selectedTeamId);
      set({
        teams,
        selectedTeamId: selectedStillExists ? state.selectedTeamId : null,
        detail: selectedStillExists && state.detail && selectedSummary
          ? { ...state.detail, team: selectedSummary }
          : null,
        isDetailLoading: selectedStillExists ? state.isDetailLoading : false,
        detailRequestId: selectedStillExists ? state.detailRequestId : 0,
        isLoading: false,
        error: null,
        listRequestId: 0,
      });
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
        selectedTeamId: team.id,
        detail: { team, members: [] },
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

  async selectTeam(port, teamId) {
    const current = get();
    if (!current.ownerUserId || !port.loadTeam || !current.teams.some((team) => team.id === teamId)) return;
    const ownerUserId = current.ownerUserId;
    const requestId = current.sequence + 1;
    set({
      selectedTeamId: teamId,
      detail: current.detail?.team.id === teamId ? current.detail : null,
      sequence: requestId,
      detailRequestId: requestId,
      isDetailLoading: true,
      error: null,
    });
    try {
      const detail = await port.loadTeam(ownerUserId, teamId);
      const state = get();
      if (state.ownerUserId !== ownerUserId
        || state.selectedTeamId !== teamId
        || state.detailRequestId !== requestId) return;
      if (detail.team.id !== teamId) throw new Error("Agent 团队身份不一致，请重新选择团队。");
      set({
        detail,
        teams: upsertTeam(state.teams, detail.team),
        isDetailLoading: false,
        detailRequestId: 0,
      });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId
        || state.selectedTeamId !== teamId
        || state.detailRequestId !== requestId) return;
      set({ isDetailLoading: false, error: errorMessage(failure), detailRequestId: 0 });
    }
  },

  async inviteAgent(port, teamId, agentId) {
    const current = get();
    if (!current.ownerUserId
      || !port.inviteAgentToTeam
      || current.selectedTeamId !== teamId
      || current.isInviting) return false;
    const ownerUserId = current.ownerUserId;
    const requestId = current.sequence + 1;
    set({
      sequence: requestId,
      detailRequestId: 0,
      inviteRequestId: requestId,
      isDetailLoading: false,
      isInviting: true,
      error: null,
    });
    try {
      const detail = await port.inviteAgentToTeam(ownerUserId, teamId, agentId);
      const state = get();
      if (state.ownerUserId !== ownerUserId
        || state.selectedTeamId !== teamId
        || state.inviteRequestId !== requestId) return false;
      if (detail.team.id !== teamId || !detail.members.some((member) => member.agentId === agentId)) {
        throw new Error("Agent 团队成员身份不一致，请刷新后重试。");
      }
      set({
        detail,
        teams: upsertTeam(state.teams, detail.team),
        isInviting: false,
        inviteRequestId: 0,
      });
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId
        || state.selectedTeamId !== teamId
        || state.inviteRequestId !== requestId) return false;
      set({ isInviting: false, error: errorMessage(failure), inviteRequestId: 0 });
      return false;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      teams: [],
      selectedTeamId: null,
      detail: null,
      isLoading: false,
      isCreating: false,
      isDetailLoading: false,
      isInviting: false,
      error: null,
      sequence: state.sequence + 1,
      listRequestId: 0,
      mutationRequestId: 0,
      detailRequestId: 0,
      inviteRequestId: 0,
    }));
  },
}));

function upsertTeam(teams: AgentTeamSummary[], team: AgentTeamSummary) {
  return [team, ...teams.filter((candidate) => candidate.id !== team.id)];
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "Agent 团队暂时无法读取。";
}
