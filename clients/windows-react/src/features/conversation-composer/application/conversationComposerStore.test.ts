import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import type { ConversationComposerPort } from "../ports/ConversationComposerPort";
import {
  REQUIREMENT_SURVEY_CAPABILITY,
  type ComposerCapabilityCandidate,
} from "../domain/conversationComposer";
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

  it("selects and removes a trailing capability query without deleting prior text", () => {
    const store = useConversationComposerStore.getState();
    store.setDraft("请先分析 /需求");
    store.selectCapability(REQUIREMENT_SURVEY_CAPABILITY);

    expect(useConversationComposerStore.getState()).toMatchObject({
      draft: "请先分析 ",
      explicitCapabilitySelection: REQUIREMENT_SURVEY_CAPABILITY.selection,
      highlightedCapabilityId: null,
    });
    useConversationComposerStore.getState().setExplicitCapabilitySelection(null);
    expect(useConversationComposerStore.getState().explicitCapabilitySelection).toBeNull();
  });

  it("suppresses suggestions until the draft changes and cycles highlighted candidates", () => {
    const plugin: ComposerCapabilityCandidate = {
      id: "plugin:document:document-mcp",
      selection: {
        kind: "plugin",
        displayName: "Document",
        pluginKey: "document",
        componentKey: "document-mcp",
      },
      subtitle: "Create a document",
    };
    const candidates = [REQUIREMENT_SURVEY_CAPABILITY, plugin];
    useConversationComposerStore.getState().setDraft("/", candidates);
    expect(useConversationComposerStore.getState().highlightedCapabilityId).toBe("requirement_survey");

    useConversationComposerStore.getState().moveCapabilityHighlight("down", candidates);
    expect(useConversationComposerStore.getState().highlightedCapabilityId).toBe(plugin.id);
    useConversationComposerStore.getState().moveCapabilityHighlight("down", candidates);
    expect(useConversationComposerStore.getState().highlightedCapabilityId).toBe("requirement_survey");

    useConversationComposerStore.getState().dismissCapabilitySuggestions();
    expect(useConversationComposerStore.getState()).toMatchObject({
      capabilitySuggestionsSuppressed: true,
      highlightedCapabilityId: null,
    });
    useConversationComposerStore.getState().setDraft("/需", candidates);
    expect(useConversationComposerStore.getState()).toMatchObject({
      capabilitySuggestionsSuppressed: false,
      highlightedCapabilityId: "requirement_survey",
    });
  });

  it("restores the submitted capability when sending fails", async () => {
    const port: ConversationComposerPort = {
      sendDraft: vi.fn().mockRejectedValue(new Error("offline")),
    };
    const store = useConversationComposerStore.getState();
    store.setDraft("需要调研");
    store.setExplicitCapabilitySelection(REQUIREMENT_SURVEY_CAPABILITY.selection);

    await store.send(port, "owner-1", "conversation-1", snapshot, async () => undefined);

    expect(useConversationComposerStore.getState()).toMatchObject({
      draft: "需要调研",
      explicitCapabilitySelection: REQUIREMENT_SURVEY_CAPABILITY.selection,
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
