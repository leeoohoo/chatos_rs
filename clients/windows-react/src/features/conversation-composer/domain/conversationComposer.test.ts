import { describe, expect, it } from "vitest";
import type { ConversationHistorySnapshot } from "../../conversation-history/domain/conversationHistory";
import {
  filterCapabilityCandidates,
  planConversationSend,
  recoverComposerDraft,
  removeTrailingCapabilityQuery,
  REQUIREMENT_SURVEY_CAPABILITY,
  trailingCapabilityQuery,
} from "./conversationComposer";

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
    expect(planConversationSend("  ", [], null, history(), false).kind).toBe("ignore");
    expect(planConversationSend("hello", [], null, history(), true).kind).toBe("ignore");
  });

  it("routes plain text to the active Turn as guidance", () => {
    expect(planConversationSend(" continue ", [], null, history("running"), false)).toEqual({
      kind: "guidance",
      submission: { content: "continue", attachmentIds: [], explicitCapabilitySelection: null },
      turnId: "turn-1",
    });
  });

  it("blocks an explicit capability while a Turn is active", () => {
    const result = planConversationSend(
      "make a document",
      [],
      { kind: "plugin", displayName: "Document", pluginKey: "document" },
      history("running"),
      false,
    );
    expect(result.kind).toBe("blocked");
  });

  it("starts a new Turn after the previous one completed", () => {
    expect(planConversationSend("next", [], null, history("succeeded"), false).kind).toBe("newTurn");
  });

  it("allows an attachment-only submission", () => {
    expect(planConversationSend("", ["attachment-1"], null, history("succeeded"), false)).toEqual({
      kind: "newTurn",
      submission: {
        content: "",
        attachmentIds: ["attachment-1"],
        explicitCapabilitySelection: null,
      },
    });
  });

  it("does not overwrite text typed while a failed send was pending", () => {
    expect(recoverComposerDraft("new text", "failed text")).toBe("new text");
    expect(recoverComposerDraft("", "failed text")).toBe("failed text");
  });
});

describe("composer capability syntax", () => {
  it("recognizes a bare slash and a trailing query at a natural-language boundary", () => {
    expect(trailingCapabilityQuery("/")).toEqual({ start: 0, end: 1, value: "" });
    const query = trailingCapabilityQuery("请帮我处理 /需求");
    expect(query).toEqual({ start: 6, end: 9, value: "需求" });
    expect(removeTrailingCapabilityQuery("请帮我处理 /需求", query!)).toBe("请帮我处理 ");
  });

  it("does not treat paths, URLs, or closed selections as capability queries", () => {
    expect(trailingCapabilityQuery("/Users/me/file")).toBeNull();
    expect(trailingCapabilityQuery("/Document 帮我处理")).toBeNull();
    expect(trailingCapabilityQuery("https://example.com/")).toBeNull();
    expect(trailingCapabilityQuery(`/${"a".repeat(81)}`)).toBeNull();
  });

  it("filters the executable catalog by display name and subtitle", () => {
    const empty = trailingCapabilityQuery("/")!;
    expect(filterCapabilityCandidates([REQUIREMENT_SURVEY_CAPABILITY], empty)).toEqual([
      REQUIREMENT_SURVEY_CAPABILITY,
    ]);
    expect(filterCapabilityCandidates(
      [REQUIREMENT_SURVEY_CAPABILITY],
      trailingCapabilityQuery("/范围")!,
    )).toEqual([REQUIREMENT_SURVEY_CAPABILITY]);
    expect(filterCapabilityCandidates(
      [REQUIREMENT_SURVEY_CAPABILITY],
      trailingCapabilityQuery("/Document")!,
    )).toEqual([]);
  });
});
