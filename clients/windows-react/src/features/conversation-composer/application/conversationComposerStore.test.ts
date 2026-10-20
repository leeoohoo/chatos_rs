import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";
import { useConversationComposerStore } from "./conversationComposerStore";

const snapshot: ConversationHistorySnapshot = {
  conversation: {
    conversation_id: "conversation-1",
    owner_user_id: "owner-1",
    title: "Project",
    resource: null,
    version: 1,
    created_at_unix_ms: 1,
    updated_at_unix_ms: 1,
  },
  turns: [],
  messages: [],
  attachments: [],
  next_before_ordinal: null,
};

describe("conversation composer store", () => {
  beforeEach(() => useConversationComposerStore.getState().reset("owner-1", "conversation-1"));

  it("clears accepted input and refreshes history", async () => {
    const port: ConversationComposerPort = {
      sendDraft: vi.fn().mockResolvedValue({
        deliveryKind: "newTurn",
        turnId: "turn-2",
        userMessageId: "message-2",
      }),
    };
    const refreshed = vi.fn().mockResolvedValue(undefined);
    useConversationComposerStore.getState().setDraft(" hello ");

    await useConversationComposerStore.getState().send(
      port,
      "owner-1",
      "conversation-1",
      snapshot,
      refreshed,
    );

    expect(port.sendDraft).toHaveBeenCalledWith(expect.objectContaining({
      guidanceTurnId: null,
      submission: { content: "hello", explicitCapabilitySelection: null },
    }));
    expect(refreshed).toHaveBeenCalledOnce();
    expect(useConversationComposerStore.getState()).toMatchObject({
      draft: "",
      isSending: false,
      error: null,
    });
  });

  it("restores a failed draft without replacing newer typing", async () => {
    let reject!: (reason: Error) => void;
    const port: ConversationComposerPort = {
      sendDraft: () => new Promise((_, failure) => { reject = failure; }),
    };
    useConversationComposerStore.getState().setDraft("failed text");
    const pending = useConversationComposerStore.getState().send(
      port,
      "owner-1",
      "conversation-1",
      snapshot,
      async () => undefined,
    );
    useConversationComposerStore.getState().setDraft("new text");
    reject(new Error("offline"));
    await pending;

    expect(useConversationComposerStore.getState()).toMatchObject({
      draft: "new text",
      isSending: false,
      error: "offline",
    });
  });

  it("rejects a late response after the conversation changes", async () => {
    let resolve!: (value: Awaited<ReturnType<ConversationComposerPort["sendDraft"]>>) => void;
    const port: ConversationComposerPort = {
      sendDraft: () => new Promise((accepted) => { resolve = accepted; }),
    };
    useConversationComposerStore.getState().setDraft("old");
    const pending = useConversationComposerStore.getState().send(
      port,
      "owner-1",
      "conversation-1",
      snapshot,
      async () => undefined,
    );
    useConversationComposerStore.getState().reset("owner-1", "conversation-2");
    useConversationComposerStore.getState().setDraft("new");
    resolve({ deliveryKind: "newTurn", turnId: "turn-old", userMessageId: "message-old" });
    await pending;

    expect(useConversationComposerStore.getState()).toMatchObject({
      conversationId: "conversation-2",
      draft: "new",
      isSending: false,
    });
  });
});
