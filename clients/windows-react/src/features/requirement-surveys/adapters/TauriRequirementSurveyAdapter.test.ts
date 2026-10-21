import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriRequirementSurveyAdapter } from "./TauriRequirementSurveyAdapter";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("TauriRequirementSurveyAdapter", () => {
  beforeEach(() => vi.mocked(invoke).mockReset());

  it("uses owner/project scoped list and exact versioned resolution commands", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([fixture()]);
    const adapter = new TauriRequirementSurveyAdapter();
    await expect(adapter.listProjectRequirementSurveys("owner-1", "project-1"))
      .resolves.toEqual([fixture()]);
    expect(invoke).toHaveBeenLastCalledWith("list_project_requirement_surveys", {
      ownerUserId: "owner-1", projectId: "project-1",
    });

    vi.mocked(invoke).mockResolvedValueOnce(fixture({
      status: "resolved", version: 2, answers: { goal: "Alpha" },
      updatedAtUnixMs: 30, resolvedAtUnixMs: 30,
    }));
    await adapter.resolveProjectRequirementSurvey(
      "owner-1", "project-1", "survey-1", 1, { goal: "Alpha" },
    );
    expect(invoke).toHaveBeenLastCalledWith("resolve_project_requirement_survey", {
      input: {
        ownerUserId: "owner-1", projectId: "project-1", surveyId: "survey-1",
        expectedVersion: 1, answers: { goal: "Alpha" },
      },
    });
  });

  it("rejects native projections with unknown path or secret fields", async () => {
    const adapter = new TauriRequirementSurveyAdapter();
    vi.mocked(invoke).mockResolvedValueOnce([{ ...fixture(), absolutePath: "C:\\private" }]);
    await expect(adapter.listProjectRequirementSurveys("owner-1", "project-1")).rejects.toThrow();
    vi.mocked(invoke).mockResolvedValueOnce({ ...fixture(), accessToken: "secret" });
    await expect(adapter.resolveProjectRequirementSurvey(
      "owner-1", "project-1", "survey-1", 1, { goal: "Alpha" },
    )).rejects.toThrow();
  });
});

function fixture(overrides: Record<string, unknown> = {}) {
  return {
    surveyId: "survey-1", projectId: "project-1", sourceConversationId: "conversation-1",
    sourceRunId: "run-1", sourceTaskId: null, title: "项目目标", description: null,
    questions: [{ questionId: "goal", prompt: "目标？", responseKind: "text", required: true, options: [] }],
    answers: null, status: "open", version: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    resolvedAtUnixMs: null, ...overrides,
  };
}
