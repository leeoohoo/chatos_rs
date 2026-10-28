import { beforeEach, describe, expect, it } from "vitest";
import conversationHistory from "../../../../../contracts/local-agent/v50/fixtures/conversation-history-success.json";
import { conversationHistoryResponseSchema } from "../../../contracts/conversationRunContract";
import type { ConversationHistorySnapshot } from "../domain/conversationHistory";
import type { ConversationHistoryPort } from "../ports/ConversationHistoryPort";
import { useConversationHistoryStore } from "./conversationHistoryStore";

const fixture = conversationHistoryResponseSchema.parse(conversationHistory).result.page;

describe("conversationHistoryStore", () => {
  beforeEach(() => useConversationHistoryStore.getState().reset());

  it("loads the latest ten-message contract page for one owner", async () => {
    const port = new HistoryPortStub([fixture]);

    await useConversationHistoryStore.getState().load(port, "user-1", "conversation-1");

    const phase = useConversationHistoryStore.getState().phase;
    expect(phase.kind).toBe("ready");
    if (phase.kind === "ready") {
      expect(phase.snapshot.messages.map((message) => message.message_id)).toEqual([
        "message-user-1",
        "message-assistant-1",
      ]);
    }
    expect(port.requests).toEqual([{ ownerUserId: "user-1", conversationId: "conversation-1", beforeOrdinal: undefined }]);
  });

  it("rejects a late page after the owner changes", async () => {
    let resolve!: (value: ConversationHistorySnapshot) => void;
    const port: ConversationHistoryPort = {
      loadHistory: () => new Promise((completion) => { resolve = completion; }),
    };

    const pending = useConversationHistoryStore.getState().load(port, "user-1", "conversation-1");
    useConversationHistoryStore.getState().reset("user-2");
    resolve(fixture);
    await pending;

    expect(useConversationHistoryStore.getState().phase).toEqual({ kind: "idle" });
  });

  it("keeps a failure until an explicit retry", async () => {
    const port = new HistoryPortStub([new Error("offline"), fixture]);

    await useConversationHistoryStore.getState().load(port, "user-1", "conversation-1");
    await useConversationHistoryStore.getState().load(port, "user-1", "conversation-1");
    expect(port.requests).toHaveLength(1);

    await useConversationHistoryStore.getState().load(port, "user-1", "conversation-1", true);
    expect(port.requests).toHaveLength(2);
    expect(useConversationHistoryStore.getState().phase.kind).toBe("ready");
  });

  it("merges older messages by identity and ordinal", async () => {
    const latest = { ...fixture, next_before_ordinal: 1 };
    const older = {
      ...fixture,
      messages: [{ ...fixture.messages[0]!, message_id: "message-older", ordinal: 0 }],
      attachments: [],
      next_before_ordinal: null,
    };
    const port = new HistoryPortStub([latest, older]);

    await useConversationHistoryStore.getState().load(port, "user-1", "conversation-1");
    await useConversationHistoryStore.getState().loadOlder(port);

    const phase = useConversationHistoryStore.getState().phase;
    expect(phase.kind).toBe("ready");
    if (phase.kind === "ready") {
      expect(phase.snapshot.messages.map((message) => message.ordinal)).toEqual([0, 1, 2]);
      expect(phase.snapshot.next_before_ordinal).toBeNull();
    }
  });

  it("refreshes the latest page without discarding older history", async () => {
    const latest = { ...fixture, next_before_ordinal: 1 };
    const olderMessage = { ...fixture.messages[0]!, message_id: "message-older", ordinal: 0 };
    const older = { ...fixture, messages: [olderMessage], attachments: [], next_before_ordinal: null };
    const refreshed = {
      ...fixture,
      conversation: { ...fixture.conversation, version: fixture.conversation.version + 1 },
      messages: [
        ...fixture.messages,
        { ...fixture.messages[1]!, message_id: "message-new", ordinal: 3 },
      ],
    };
    const port = new HistoryPortStub([latest, older, refreshed]);

    await useConversationHistoryStore.getState().load(port, "user-1", "conversation-1");
    await useConversationHistoryStore.getState().loadOlder(port);
    await useConversationHistoryStore.getState().refreshLatest(port);

    const phase = useConversationHistoryStore.getState().phase;
    expect(phase.kind).toBe("ready");
    if (phase.kind === "ready") {
      expect(phase.snapshot.messages.map((message) => message.ordinal)).toEqual([0, 1, 2, 3]);
      expect(phase.snapshot.next_before_ordinal).toBeNull();
      expect(phase.isRefreshing).toBe(false);
    }
  });
});

class HistoryPortStub implements ConversationHistoryPort {
  readonly requests: Array<{ ownerUserId: string; conversationId: string; beforeOrdinal?: number }> = [];

  constructor(private readonly responses: Array<ConversationHistorySnapshot | Error>) {}

  async loadHistory(ownerUserId: string, conversationId: string, beforeOrdinal?: number) {
    this.requests.push({ ownerUserId, conversationId, beforeOrdinal });
    const response = this.responses.shift();
    if (response instanceof Error) throw response;
    if (!response) throw new Error("missing test response");
    return response;
  }
}
