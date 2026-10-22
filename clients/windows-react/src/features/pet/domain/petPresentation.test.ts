import { describe, expect, it } from "vitest";
import { petPresentationSchema } from "./petPresentation";

describe("pet presentation", () => {
  it("accepts only the bounded safe native projection", () => {
    const valid = {
      language: "zh-CN",
      animationState: "waiting",
      primaryActivity: { source: "task_execution", kind: "waiting_for_user", detailCode: "waiting_user", updatedAtUnixMs: 1 },
      activeWorkCount: 2,
      attentionCount: 1,
    };
    expect(petPresentationSchema.safeParse(valid).success).toBe(true);
    expect(petPresentationSchema.safeParse({ ...valid, runId: "must-not-cross-the-window-boundary" }).success).toBe(false);
  });
});
