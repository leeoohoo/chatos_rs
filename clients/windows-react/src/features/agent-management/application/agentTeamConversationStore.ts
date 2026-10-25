import { create } from "zustand";
import type { AgentTeamConversationSnapshot } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface AgentTeamConversationState {
  ownerUserId: string | null;
  teamId: string | null;
  snapshot: AgentTeamConversationSnapshot | null;
  isLoading: boolean;
  isLoadingOlder: boolean;
  isSending: boolean;
  isCycling: boolean;
  error: string | null;
  sequence: number;
  requestId: number;
  activate(port: AgentManagementPort, ownerUserId: string, teamId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  loadOlder(port: AgentManagementPort): Promise<boolean>;
  send(
    port: AgentManagementPort,
    content: string,
    mentionedAgentIds: string[],
    attachmentIds?: string[],
  ): Promise<boolean>;
  reset(): void;
}

export const useAgentTeamConversationStore = create<AgentTeamConversationState>((set, get) => ({
  ownerUserId: null,
  teamId: null,
  snapshot: null,
  isLoading: false,
  isLoadingOlder: false,
  isSending: false,
  isCycling: false,
  error: null,
  sequence: 0,
  requestId: 0,

  async activate(port, ownerInput, teamInput) {
    const ownerUserId = ownerInput.trim();
    const teamId = teamInput.trim();
    if (!ownerUserId || !teamId) {
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
        isLoadingOlder: false,
        isSending: false,
        isCycling: false,
        error: null,
        sequence: current.sequence + 1,
        requestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.teamId || current.isCycling || current.isSending
      || !supportsConversation(port)) return;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const requestId = current.sequence + 1;
    set({
      sequence: requestId,
      requestId,
      isLoading: current.snapshot == null,
      isCycling: true,
      error: null,
    });
    try {
      const snapshot = await cycleTeamConversation(port, ownerUserId, teamId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      set({
        snapshot: mergeConversationSnapshots(state.snapshot, snapshot, state.snapshot?.hasOlder),
        isLoading: false,
        isCycling: false,
        requestId: 0,
      });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return;
      set({ isLoading: false, isCycling: false, error: errorMessage(failure), requestId: 0 });
    }
  },

  async loadOlder(port) {
    const current = get();
    const firstMessageId = current.snapshot?.messages[0]?.id;
    if (!current.ownerUserId || !current.teamId || !firstMessageId
      || !current.snapshot?.hasOlder || current.isLoadingOlder || current.isCycling
      || current.isSending || !supportsConversation(port)) return false;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const requestId = current.sequence + 1;
    set({ sequence: requestId, requestId, isLoadingOlder: true, error: null });
    try {
      const older = await port.loadTeamConversation(ownerUserId, teamId, firstMessageId, 50);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return false;
      if (older.teamId !== teamId) throw new Error("Agent 团队消息身份不一致，请刷新后重试。");
      set({
        snapshot: mergeConversationSnapshots(state.snapshot, older, older.hasOlder),
        isLoadingOlder: false,
        requestId: 0,
      });
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return false;
      set({ isLoadingOlder: false, error: errorMessage(failure), requestId: 0 });
      return false;
    }
  },

  async send(port, contentInput, mentionedAgentIds, attachmentIds = []) {
    const current = get();
    const content = contentInput.trim();
    if (!current.ownerUserId
      || !current.teamId
      || (!content && attachmentIds.length === 0)
      || current.isSending
      || current.isCycling
      || !supportsConversation(port)) return false;
    const ownerUserId = current.ownerUserId;
    const teamId = current.teamId;
    const requestId = current.sequence + 1;
    let accepted = false;
    set({ sequence: requestId, requestId, isSending: true, error: null });
    try {
      const queued = await port.queueTeamMessage(
        ownerUserId,
        teamId,
        content,
        mentionedAgentIds,
        attachmentIds,
      );
      if (queued.teamId !== teamId) throw new Error("Agent 团队消息身份不一致，请刷新后重试。");
      accepted = true;
      const snapshot = await cycleTeamConversation(port, ownerUserId, teamId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return false;
      set({
        snapshot: mergeConversationSnapshots(state.snapshot, snapshot, state.snapshot?.hasOlder),
        isSending: false,
        requestId: 0,
      });
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.teamId !== teamId || state.requestId !== requestId) return false;
      set({ isSending: false, error: errorMessage(failure), requestId: 0 });
      return accepted;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      teamId: null,
      snapshot: null,
      isLoading: false,
      isLoadingOlder: false,
      isSending: false,
      isCycling: false,
      error: null,
      sequence: state.sequence + 1,
      requestId: 0,
    }));
  },
}));

function supportsConversation(port: AgentManagementPort): port is AgentManagementPort & Required<Pick<
  AgentManagementPort,
  "loadTeamConversation" | "queueTeamMessage" | "dispatchNextTeamDelivery" | "loadTeamMemberActivity"
>> {
  return Boolean(
    port.loadTeamConversation
    && port.queueTeamMessage
    && port.dispatchNextTeamDelivery
    && port.loadTeamMemberActivity,
  );
}

async function cycleTeamConversation(
  port: AgentManagementPort & Required<Pick<
    AgentManagementPort,
    "loadTeamConversation" | "queueTeamMessage" | "dispatchNextTeamDelivery" | "loadTeamMemberActivity"
  >>,
  ownerUserId: string,
  teamId: string,
) {
  let snapshot = await port.loadTeamConversation(ownerUserId, teamId, null, 50);
  if (snapshot.teamId !== teamId) throw new Error("Agent 团队消息身份不一致，请刷新后重试。");
  const runningAgentIds = [...new Set(snapshot.deliveries
    .filter((delivery) => delivery.status === "running")
    .map((delivery) => delivery.targetAgentId))];
  if (runningAgentIds.length > 0) {
    await Promise.all(runningAgentIds.map(async (agentId) => {
      const activity = await port.loadTeamMemberActivity(ownerUserId, teamId, agentId);
      if (activity.roomId !== teamId || activity.agentId !== agentId) {
        throw new Error("团队 Agent 运行身份不一致，请刷新后重试。");
      }
    }));
  }
  let dispatched = false;
  for (let count = 0; count < 100; count += 1) {
    const delivery = await port.dispatchNextTeamDelivery(ownerUserId, teamId);
    if (!delivery) break;
    if (delivery.teamId !== teamId) throw new Error("Agent Delivery 团队身份不一致，请刷新后重试。");
    dispatched = true;
  }
  if (runningAgentIds.length > 0 || dispatched) {
    snapshot = await port.loadTeamConversation(ownerUserId, teamId, null, 50);
    if (snapshot.teamId !== teamId) throw new Error("Agent 团队消息身份不一致，请刷新后重试。");
  }
  return snapshot;
}

function mergeConversationSnapshots(
  current: AgentTeamConversationSnapshot | null,
  incoming: AgentTeamConversationSnapshot,
  hasOlder = incoming.hasOlder,
): AgentTeamConversationSnapshot {
  if (!current || current.teamId !== incoming.teamId) return incoming;
  const messages = new Map(current.messages.map((message) => [message.id, message]));
  for (const message of incoming.messages) messages.set(message.id, message);
  const deliveries = new Map(current.deliveries.map((delivery) => [delivery.id, delivery]));
  for (const delivery of incoming.deliveries) deliveries.set(delivery.id, delivery);
  return {
    teamId: incoming.teamId,
    messages: [...messages.values()].sort((left, right) => (
      left.createdAtUnixMs - right.createdAtUnixMs || left.id.localeCompare(right.id)
    )),
    deliveries: [...deliveries.values()].sort((left, right) => (
      right.createdAtUnixMs - left.createdAtUnixMs || right.id.localeCompare(left.id)
    )),
    hasOlder,
  };
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "Agent 团队消息暂时不可用。";
}
