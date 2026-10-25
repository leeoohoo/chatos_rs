import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentTeamConversationSnapshot } from "../domain/agentManagement";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useAgentTeamConversationStore } from "./agentTeamConversationStore";

describe("agent team conversation store", () => {
  beforeEach(() => useAgentTeamConversationStore.getState().reset());

  it("loads the exact team snapshot and dispatches queued work", async () => {
    const initial = snapshot();
    const running = snapshot({
      messages: [message("message-1")],
      deliveries: [delivery("delivery-1", "running")],
    });
    const port = stubPort({
      loadTeamConversation: vi.fn().mockResolvedValueOnce(initial).mockResolvedValueOnce(initial)
        .mockResolvedValueOnce(running),
      queueTeamMessage: vi.fn(async () => ({
        messageId: "message-1", teamId: TEAM_ID, createdAtUnixMs: 10,
        deliveries: [{ deliveryId: "delivery-1", targetAgentId: "agent-1", triggerKind: "default_agent" as const }],
      })),
      dispatchNextTeamDelivery: vi.fn().mockResolvedValueOnce(null).mockResolvedValueOnce({
        deliveryId: "delivery-1", teamId: TEAM_ID, agentId: "agent-1", turnId: "turn-1", runId: "run-1",
      }).mockResolvedValueOnce(null),
    });
    await useAgentTeamConversationStore.getState().activate(port, "owner-1", TEAM_ID);
    expect(await useAgentTeamConversationStore.getState().send(port, " 开始实现 ", [])).toBe(true);
    expect(port.queueTeamMessage).toHaveBeenCalledWith("owner-1", TEAM_ID, "开始实现", [], []);
    expect(port.dispatchNextTeamDelivery).toHaveBeenCalledTimes(3);
    expect(port.loadTeamConversation).toHaveBeenCalledWith("owner-1", TEAM_ID, null, 50);
    expect(useAgentTeamConversationStore.getState().snapshot).toEqual(running);
  });

  it("accepts an attachment-only message and does not invite a duplicate retry after refresh fails", async () => {
    const port = stubPort({
      loadTeamConversation: vi.fn().mockResolvedValueOnce(snapshot())
        .mockRejectedValueOnce(new Error("刷新暂时失败")),
      queueTeamMessage: vi.fn(async () => ({
        messageId: "message-attachment", teamId: TEAM_ID, createdAtUnixMs: 10,
        deliveries: [{ deliveryId: "delivery-attachment", targetAgentId: "agent-1", triggerKind: "default_agent" as const }],
      })),
    });
    await useAgentTeamConversationStore.getState().activate(port, "owner-1", TEAM_ID);
    expect(await useAgentTeamConversationStore.getState().send(
      port, "", [], ["attachment-1"],
    )).toBe(true);
    expect(port.queueTeamMessage).toHaveBeenCalledWith(
      "owner-1", TEAM_ID, "", [], ["attachment-1"],
    );
    expect(useAgentTeamConversationStore.getState().error).toBe("刷新暂时失败");
  });

  it("reconciles running Agent lanes before dispatching more work", async () => {
    const running = snapshot({
      messages: [message("message-1")],
      deliveries: [delivery("delivery-1", "running")],
    });
    const completed = snapshot({
      messages: [message("message-1"), { ...message("answer-1"), senderKind: "agent", senderId: "agent-1" }],
      deliveries: [{ ...delivery("delivery-1", "completed"), responseMessageId: "answer-1", completedAtUnixMs: 20 }],
    });
    const port = stubPort({
      loadTeamConversation: vi.fn().mockResolvedValueOnce(running).mockResolvedValueOnce(completed),
    });
    await useAgentTeamConversationStore.getState().activate(port, "owner-1", TEAM_ID);
    expect(port.loadTeamMemberActivity).toHaveBeenCalledWith("owner-1", TEAM_ID, "agent-1");
    expect(useAgentTeamConversationStore.getState().snapshot).toEqual(completed);
  });

  it("prepends older messages without losing the latest page", async () => {
    const recent = snapshot({
      messages: [message("message-2"), message("message-3")],
      hasOlder: true,
    });
    const older = snapshot({
      messages: [message("message-1")],
      hasOlder: false,
    });
    const port = stubPort({
      loadTeamConversation: vi.fn().mockResolvedValueOnce(recent).mockResolvedValueOnce(older),
    });
    await useAgentTeamConversationStore.getState().activate(port, "owner-1", TEAM_ID);
    expect(await useAgentTeamConversationStore.getState().loadOlder(port)).toBe(true);
    expect(port.loadTeamConversation).toHaveBeenLastCalledWith("owner-1", TEAM_ID, "message-2", 50);
    expect(useAgentTeamConversationStore.getState().snapshot?.messages.map(({ id }) => id)).toEqual([
      "message-1", "message-2", "message-3",
    ]);
    expect(useAgentTeamConversationStore.getState().snapshot?.hasOlder).toBe(false);
  });

  it("does not let a late previous team replace the active team", async () => {
    const late = deferred<AgentTeamConversationSnapshot>();
    const otherId = "22222222-2222-4222-8222-222222222222";
    const port = stubPort({
      loadTeamConversation: vi.fn(async (_owner, teamId) => teamId === TEAM_ID
        ? late.promise
        : snapshot({ teamId: otherId })),
    });
    const stale = useAgentTeamConversationStore.getState().activate(port, "owner-1", TEAM_ID);
    await useAgentTeamConversationStore.getState().activate(port, "owner-1", otherId);
    late.resolve(snapshot());
    await stale;
    expect(useAgentTeamConversationStore.getState().teamId).toBe(otherId);
    expect(useAgentTeamConversationStore.getState().snapshot?.teamId).toBe(otherId);
  });
});

const TEAM_ID = "11111111-1111-4111-8111-111111111111";

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => []), loadEditorCatalog: vi.fn(), loadProfileEditor: vi.fn(),
    saveProfile: vi.fn(), pickAvatar: vi.fn(async () => null), loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined), discardAvatarPresentation: vi.fn(async () => undefined),
    loadTeamConversation: vi.fn(async () => snapshot()),
    queueTeamMessage: vi.fn(),
    dispatchNextTeamDelivery: vi.fn(async () => null),
    loadTeamMemberActivity: vi.fn(async (_owner, teamId, agentId) => ({
      roomId: teamId, agentId, conversationId: `agent-team-${teamId}-${agentId}`,
      conversationVersion: 1, hasActiveDelivery: false, items: [],
    })),
    ...overrides,
  };
}

function snapshot(overrides: Partial<AgentTeamConversationSnapshot> = {}): AgentTeamConversationSnapshot {
  return { teamId: TEAM_ID, messages: [], deliveries: [], ...overrides };
}

function message(id: string): AgentTeamConversationSnapshot["messages"][number] {
  return {
    id, senderKind: "human", senderId: "owner-1", content: "开始实现",
    mentionedAgentIds: [], attachments: [], createdAtUnixMs: 10,
  };
}

function delivery(id: string, status: "running" | "completed"): AgentTeamConversationSnapshot["deliveries"][number] {
  return {
    id, messageId: "message-1", targetAgentId: "agent-1", triggerKind: "default_agent", status,
    responseMessageId: null, lastError: null, createdAtUnixMs: 10, completedAtUnixMs: null,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}
