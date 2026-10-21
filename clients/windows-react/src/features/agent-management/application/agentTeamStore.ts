import { create } from "zustand";
import type {
  AgentTeamDetail,
  AgentTeamSummary,
  CreateAgentTeamInput,
  UpdateAgentTeamMemberInput,
} from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

export type AgentTeamMemberMutation =
  | { kind: "update"; input: UpdateAgentTeamMemberInput }
  | { kind: "setDefault" }
  | { kind: "setProjectManager" }
  | { kind: "remove" };

interface AgentTeamState {
  ownerUserId: string | null;
  teams: AgentTeamSummary[];
  selectedTeamId: string | null;
  detail: AgentTeamDetail | null;
  isLoading: boolean;
  isCreating: boolean;
  isDetailLoading: boolean;
  isInviting: boolean;
  isMutatingMember: boolean;
  mutatingAgentId: string | null;
  error: string | null;
  sequence: number;
  listRequestId: number;
  mutationRequestId: number;
  detailRequestId: number;
  inviteRequestId: number;
  memberMutationRequestId: number;
  activate(port: AgentManagementPort, ownerUserId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  createTeam(port: AgentManagementPort, input: CreateAgentTeamInput): Promise<AgentTeamSummary | null>;
  selectTeam(port: AgentManagementPort, teamId: string): Promise<void>;
  inviteAgent(port: AgentManagementPort, teamId: string, agentId: string): Promise<boolean>;
  mutateMember(
    port: AgentManagementPort,
    teamId: string,
    agentId: string,
    mutation: AgentTeamMemberMutation,
  ): Promise<boolean>;
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
  isMutatingMember: false,
  mutatingAgentId: null,
  error: null,
  sequence: 0,
  listRequestId: 0,
  mutationRequestId: 0,
  detailRequestId: 0,
  inviteRequestId: 0,
  memberMutationRequestId: 0,

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
        isMutatingMember: false,
        mutatingAgentId: null,
        error: null,
        sequence: current.sequence + 1,
        listRequestId: 0,
        mutationRequestId: 0,
        detailRequestId: 0,
        inviteRequestId: 0,
        memberMutationRequestId: 0,
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
        isInviting: selectedStillExists ? state.isInviting : false,
        inviteRequestId: selectedStillExists ? state.inviteRequestId : 0,
        isMutatingMember: selectedStillExists ? state.isMutatingMember : false,
        mutatingAgentId: selectedStillExists ? state.mutatingAgentId : null,
        memberMutationRequestId: selectedStillExists ? state.memberMutationRequestId : 0,
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
      inviteRequestId: 0,
      memberMutationRequestId: 0,
      isDetailLoading: true,
      isInviting: false,
      isMutatingMember: false,
      mutatingAgentId: null,
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

  async mutateMember(port, teamId, agentId, mutation) {
    const current = get();
    if (!current.ownerUserId
      || current.selectedTeamId !== teamId
      || current.detail?.team.id !== teamId
      || !current.detail.members.some((member) => member.agentId === agentId)
      || current.isInviting
      || current.isMutatingMember) return false;
    const ownerUserId = current.ownerUserId;
    const request = memberMutationRequest(port, ownerUserId, teamId, agentId, mutation);
    if (!request) return false;
    const requestId = current.sequence + 1;
    set({
      sequence: requestId,
      detailRequestId: 0,
      memberMutationRequestId: requestId,
      isDetailLoading: false,
      isMutatingMember: true,
      mutatingAgentId: agentId,
      error: null,
    });
    try {
      const detail = await request;
      const state = get();
      if (state.ownerUserId !== ownerUserId
        || state.selectedTeamId !== teamId
        || state.memberMutationRequestId !== requestId) return false;
      validateMemberMutationResult(detail, teamId, agentId, mutation);
      set({
        detail,
        teams: upsertTeam(state.teams, detail.team),
        isMutatingMember: false,
        mutatingAgentId: null,
        memberMutationRequestId: 0,
      });
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId
        || state.selectedTeamId !== teamId
        || state.memberMutationRequestId !== requestId) return false;
      set({
        isMutatingMember: false,
        mutatingAgentId: null,
        error: errorMessage(failure),
        memberMutationRequestId: 0,
      });
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
      isMutatingMember: false,
      mutatingAgentId: null,
      error: null,
      sequence: state.sequence + 1,
      listRequestId: 0,
      mutationRequestId: 0,
      detailRequestId: 0,
      inviteRequestId: 0,
      memberMutationRequestId: 0,
    }));
  },
}));

function upsertTeam(teams: AgentTeamSummary[], team: AgentTeamSummary) {
  return [team, ...teams.filter((candidate) => candidate.id !== team.id)];
}

function memberMutationRequest(
  port: AgentManagementPort,
  ownerUserId: string,
  teamId: string,
  agentId: string,
  mutation: AgentTeamMemberMutation,
) {
  switch (mutation.kind) {
    case "update":
      return port.updateTeamMember?.(ownerUserId, teamId, agentId, mutation.input) ?? null;
    case "setDefault":
      return port.setTeamDefaultAgent?.(ownerUserId, teamId, agentId) ?? null;
    case "setProjectManager":
      return port.setTeamProjectManager?.(ownerUserId, teamId, agentId) ?? null;
    case "remove":
      return port.removeAgentFromTeam?.(ownerUserId, teamId, agentId) ?? null;
  }
}

function validateMemberMutationResult(
  detail: AgentTeamDetail,
  teamId: string,
  agentId: string,
  mutation: AgentTeamMemberMutation,
) {
  if (detail.team.id !== teamId) throw new Error("Agent 团队身份不一致，请刷新后重试。");
  const containsMember = detail.members.some((member) => member.agentId === agentId);
  if (mutation.kind === "remove") {
    if (containsMember) throw new Error("Agent 移出团队结果不一致，请刷新后重试。");
    return;
  }
  if (!containsMember) throw new Error("Agent 团队成员身份不一致，请刷新后重试。");
  if (mutation.kind === "setDefault" && detail.team.defaultAgentId !== agentId) {
    throw new Error("默认 Agent 设置结果不一致，请刷新后重试。");
  }
  if (mutation.kind === "setProjectManager" && detail.team.projectManagerAgentId !== agentId) {
    throw new Error("项目经理设置结果不一致，请刷新后重试。");
  }
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "Agent 团队暂时无法读取。";
}
