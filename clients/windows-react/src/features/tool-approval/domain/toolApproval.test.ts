import { describe, expect, it } from "vitest";
import { sampleToolApproval } from "../../../test/toolApprovalFixture";
import {
  allowsToolApprovalDecision,
  toolApprovalSchema,
  toolApprovalsSchema,
} from "./toolApproval";

describe("tool approval domain", () => {
  it("accepts the safe projection and rejects raw Host fields", () => {
    const approval = sampleToolApproval();
    expect(toolApprovalsSchema.parse([approval])).toEqual([approval]);
    expect(() => toolApprovalSchema.parse({
      ...approval,
      arguments: { apiKey: "must-not-cross" },
    })).toThrow();
    expect(() => toolApprovalSchema.parse({
      ...approval,
      claimToken: "must-not-cross",
    })).toThrow();
  });

  it("only allows decisions published by durable truth", () => {
    const approval = sampleToolApproval({ availableDecisions: ["decline"] });
    expect(allowsToolApprovalDecision(approval, "decline")).toBe(true);
    expect(allowsToolApprovalDecision(approval, "accept")).toBe(false);
  });
});
