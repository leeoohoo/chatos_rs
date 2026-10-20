import { describe, expect, it } from "vitest";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import { planConversationSend, recoverComposerDraft } from "./conversationComposer";

function history(status: "running" | "succeeded" = "succeeded"): ConversationHistorySnapshot {
  return {
    conversation: {
      conversation_id: "conversation-1",
      owner_user_id: "owner-1",
      title: "Project",
      resource: null,
      version: 1,
      created_at_unix_ms: 1,
      updated_at_unix_ms: 1,
    },
    turns: [{
      turn_id: "turn-1",
      conversation_id: "conversation-1",
      user_message_id: "message-1",
      run_id: "run-1",
      status,
      created_at_unix_ms: 1,
      updated_at_unix_ms: 1,
    }],
    messages: [],
    attachments: [],
    next_before_ordinal: null,
  };
}

describe("conversation send policy", () => {
  it("ignores whitespace and duplicate submission", () => {
    expect(planConversationSend("  ", null, history(), false).kind).toBe("ignore");
    expect(planConversationSend("hello", null, history(), true).kind).toBe("ignore");
  });

  it("routes plain text to the active Turn as guidance", () => {
    expect(planConversationSend(" continue ", null, history("running"), false)).toEqual({
      kind: "guidance",
      submission: { content: "continue", explicitCapabilitySelection: null },
      turnId: "turn-1",
    });
  });

  it("blocks an explicit capability while a Turn is active", () => {
    const result = planConversationSend(
      "make a document",
      { kind: "plugin", displayName: "Document", pluginKey: "document" },
      history("running"),
      false,
    );
    expect(result.kind).toBe("blocked");
  });

  it("starts a new Turn after the previous one completed", () => {
    expect(planConversationSend("next", null, history("succeeded"), false).kind).toBe("newTurn");
  });

  it("does not overwrite text typed while a failed send was pending", () => {
    expect(recoverComposerDraft("new text", "failed text")).toBe("new text");
    expect(recoverComposerDraft("", "failed text")).toBe("failed text");
  });
});
