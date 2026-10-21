import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RequirementSurvey } from "../../requirement-surveys/domain/requirementSurvey";
import type { AgentManagementPort } from "../ports/AgentManagementPort";
import { useProjectRequirementSurveyStore } from "./projectRequirementSurveyStore";

describe("project requirement survey store", () => {
  beforeEach(() => useProjectRequirementSurveyStore.getState().reset());

  it("loads and resolves the exact project survey", async () => {
    const port = stubPort();
    await useProjectRequirementSurveyStore.getState().activate(port, "owner-1", "project-1");
    const current = useProjectRequirementSurveyStore.getState().surveys[0]!;
    expect(current.status).toBe("open");
    expect(await useProjectRequirementSurveyStore.getState().submit(port, current, { goal: "交付 Alpha" })).toBe(true);
    expect(port.resolveProjectRequirementSurvey).toHaveBeenCalledWith(
      "owner-1", "project-1", "survey-1", 1, { goal: "交付 Alpha" },
    );
    expect(useProjectRequirementSurveyStore.getState().surveys[0]?.status).toBe("resolved");
  });

  it("rejects a late response from the previous project", async () => {
    let release!: (value: RequirementSurvey[]) => void;
    const late = new Promise<RequirementSurvey[]>((resolve) => { release = resolve; });
    const port = stubPort({
      listProjectRequirementSurveys: vi.fn(async (_owner, projectId) => (
        projectId === "project-1" ? late : [{ ...survey(), projectId }]
      )),
    });
    const stale = useProjectRequirementSurveyStore.getState().activate(port, "owner-1", "project-1");
    await useProjectRequirementSurveyStore.getState().activate(port, "owner-1", "project-2");
    release([survey()]);
    await stale;
    expect(useProjectRequirementSurveyStore.getState().surveys[0]?.projectId).toBe("project-2");
  });
});

function stubPort(overrides: Partial<AgentManagementPort> = {}): AgentManagementPort {
  return {
    listProfiles: vi.fn(async () => []), loadEditorCatalog: vi.fn(), loadProfileEditor: vi.fn(),
    saveProfile: vi.fn(), pickAvatar: vi.fn(async () => null), loadAvatarHistory: vi.fn(async () => []),
    discardAvatarReferences: vi.fn(async () => undefined), discardAvatarPresentation: vi.fn(async () => undefined),
    listProjectRequirementSurveys: vi.fn(async () => [survey()]),
    resolveProjectRequirementSurvey: vi.fn(async (_owner, _project, _survey, version, answers) => ({
      ...survey(), status: "resolved" as const, version: version + 1, answers,
      updatedAtUnixMs: 30, resolvedAtUnixMs: 30,
    })),
    ...overrides,
  };
}

function survey(): RequirementSurvey {
  return {
    surveyId: "survey-1", projectId: "project-1", sourceConversationId: "conversation-1",
    sourceRunId: "run-1", sourceTaskId: null, title: "项目目标", description: "请补充目标",
    questions: [{ questionId: "goal", prompt: "需要交付什么？", responseKind: "text", required: true, options: [] }],
    answers: null, status: "open", version: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    resolvedAtUnixMs: null,
  };
}
