import { describe, expect, it } from "vitest";
import {
  approvalModePresentation,
  toolApprovalSettingsSchema,
} from "./toolApprovalSettings";

describe("tool approval settings domain", () => {
  it("accepts the bounded safe settings projection", () => {
    const snapshot = {
      defaultMode: "request_approval",
      selectedModelId: null,
      selectedThinkingLevel: null,
      models: [],
      automaticReviewerReady: false,
      automaticReviewerDetail: "审批 Agent 尚未就绪。",
      history: [],
    } as const;
    expect(toolApprovalSettingsSchema.parse(snapshot)).toEqual(snapshot);
    expect(() => toolApprovalSettingsSchema.parse({
      ...snapshot,
      credential: "must-not-cross",
    })).toThrow();
  });

  it("describes all three macOS approval modes", () => {
    expect(Object.keys(approvalModePresentation).sort()).toEqual([
      "auto_approval",
      "full_control",
      "request_approval",
    ]);
  });
});
