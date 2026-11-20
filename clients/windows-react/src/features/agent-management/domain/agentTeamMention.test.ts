import { describe, expect, it } from "vitest";
import {
  removeTrailingAgentTeamMentionQuery,
  resolveAgentTeamMentionIds,
  trailingAgentTeamMentionQuery,
  visibleAgentTeamMentionCandidates,
} from "./agentTeamMention";

const candidates = [
  { id: "agent-1", name: "阿策", subtitle: "项目经理" },
  { id: "agent-2", name: "子龙", subtitle: "技术负责人" },
];

describe("agent team mentions", () => {
  it("finds a trailing mention query only at a word boundary", () => {
    expect(trailingAgentTeamMentionQuery("请 @阿")).toEqual({ start: 2, value: "阿" });
    expect(trailingAgentTeamMentionQuery("mail@example")).toBeNull();
    expect(trailingAgentTeamMentionQuery("@阿 策")).toBeNull();
  });

  it("filters candidates and removes the active query after selection", () => {
    expect(visibleAgentTeamMentionCandidates("请 @子", candidates).map(({ id }) => id)).toEqual(["agent-2"]);
    expect(removeTrailingAgentTeamMentionQuery("请 @子")).toBe("请 ");
  });

  it("routes textual mentions and rejects ambiguous names without a selected chip", () => {
    expect(resolveAgentTeamMentionIds("请 @阿策 处理", [], candidates)).toEqual({ ids: ["agent-1"], error: null });
    const duplicate = [...candidates, { id: "agent-3", name: "阿策" }];
    expect(resolveAgentTeamMentionIds("请 @阿策 处理", [], duplicate).error).toContain("多个 Agent");
    expect(resolveAgentTeamMentionIds("请 @阿策 处理", ["agent-3"], duplicate)).toEqual({
      ids: ["agent-3"], error: null,
    });
  });
});
