import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RequirementSurvey } from "../domain/requirementSurvey";
import type { RequirementSurveyPort } from "../ports/RequirementSurveyPort";
import { useRequirementSurveyCenterStore } from "./requirementSurveyCenterStore";

describe("requirement survey center store", () => {
  beforeEach(() => useRequirementSurveyCenterStore.getState().reset());

  it("aggregates exact project results and sorts open surveys first", async () => {
    const port = stubPort({
      listProjectRequirementSurveys: vi.fn(async (_owner, projectId) => projectId === "project-1"
        ? [survey({ projectId, surveyId: "resolved", status: "resolved", version: 2, answers: { goal: "done" }, resolvedAtUnixMs: 30 })]
        : [survey({ projectId, surveyId: "open", updatedAtUnixMs: 25 })]),
    });
    await useRequirementSurveyCenterStore.getState().activate(port, "owner-1", [
      { id: "project-1", title: "Alpha" },
      { id: "project-2", title: "Beta" },
    ]);
    expect(port.listProjectRequirementSurveys).toHaveBeenCalledTimes(2);
    expect(useRequirementSurveyCenterStore.getState().surveys.map((item) => item.surveyId))
      .toEqual(["open", "resolved"]);
  });

  it("rejects late results after an owner/project identity change", async () => {
    let release!: (value: RequirementSurvey[]) => void;
    const late = new Promise<RequirementSurvey[]>((resolve) => { release = resolve; });
    const port = stubPort({
      listProjectRequirementSurveys: vi.fn(async (owner, projectId) => owner === "owner-1"
        ? late
        : [survey({ projectId, surveyId: "owner-2-survey" })]),
    });
    const stale = useRequirementSurveyCenterStore.getState().activate(
      port, "owner-1", [{ id: "project-1", title: "Alpha" }],
    );
    await useRequirementSurveyCenterStore.getState().activate(
      port, "owner-2", [{ id: "project-2", title: "Beta" }],
    );
    release([survey()]);
    await stale;
    const state = useRequirementSurveyCenterStore.getState();
    expect(state.ownerUserId).toBe("owner-2");
    expect(state.surveys.map((item) => item.surveyId)).toEqual(["owner-2-survey"]);
  });

  it("keeps successful projects visible when one project fails", async () => {
    const port = stubPort({
      listProjectRequirementSurveys: vi.fn(async (_owner, projectId) => {
        if (projectId === "project-2") throw new Error("Host unavailable");
        return [survey({ projectId })];
      }),
    });
    await useRequirementSurveyCenterStore.getState().activate(port, "owner-1", [
      { id: "project-1", title: "Alpha" }, { id: "project-2", title: "Beta" },
    ]);
    expect(useRequirementSurveyCenterStore.getState().surveys).toHaveLength(1);
    expect(useRequirementSurveyCenterStore.getState().error).toContain("1 个项目");
  });

  it("refreshes Host truth after a version conflict", async () => {
    const list = vi.fn()
      .mockResolvedValueOnce([survey()])
      .mockResolvedValueOnce([survey({ version: 2, updatedAtUnixMs: 30 })]);
    const port = stubPort({
      listProjectRequirementSurveys: list,
      resolveProjectRequirementSurvey: vi.fn(async () => { throw new Error("需求调研已经变化"); }),
    });
    await useRequirementSurveyCenterStore.getState().activate(
      port, "owner-1", [{ id: "project-1", title: "Alpha" }],
    );
    const current = useRequirementSurveyCenterStore.getState().surveys[0]!;
    await expect(useRequirementSurveyCenterStore.getState().submit(port, current, { goal: "Alpha" }))
      .resolves.toBe(false);
    expect(list).toHaveBeenCalledTimes(2);
    expect(useRequirementSurveyCenterStore.getState().surveys[0]?.version).toBe(2);
    expect(useRequirementSurveyCenterStore.getState().error).toContain("已经变化");
  });

  it("invalidates an in-flight load when the page tears down", async () => {
    let release!: (value: RequirementSurvey[]) => void;
    const port = stubPort({
      listProjectRequirementSurveys: vi.fn(async () => new Promise<RequirementSurvey[]>((resolve) => { release = resolve; })),
    });
    const pending = useRequirementSurveyCenterStore.getState().activate(
      port, "owner-1", [{ id: "project-1", title: "Alpha" }],
    );
    useRequirementSurveyCenterStore.getState().reset();
    release([survey()]);
    await pending;
    expect(useRequirementSurveyCenterStore.getState().surveys).toEqual([]);
    expect(useRequirementSurveyCenterStore.getState().ownerUserId).toBeNull();
  });
});

function stubPort(overrides: Partial<RequirementSurveyPort> = {}): RequirementSurveyPort {
  const defaults: RequirementSurveyPort = {
    listProjectRequirementSurveys: vi.fn(async () => [survey()]),
    resolveProjectRequirementSurvey: vi.fn(async (_owner, _project, _survey, version, answers) => survey({
      status: "resolved", version: version + 1, answers, updatedAtUnixMs: 30, resolvedAtUnixMs: 30,
    })),
    loadFollowUp: vi.fn(async () => followUp()),
    confirmPlan: vi.fn(async () => plan({ confirmedAtUnixMs: 40 })),
    adoptPlan: vi.fn(async () => plan({ confirmedAtUnixMs: 40 })),
  };
  return { ...defaults, ...overrides } as RequirementSurveyPort;
}

function survey(overrides: Partial<RequirementSurvey> = {}): RequirementSurvey {
  return {
    surveyId: "survey-1", projectId: "project-1", sourceConversationId: "conversation-1",
    sourceRunId: "run-1", sourceTaskId: null, title: "项目目标", description: "请补充目标",
    questions: [{ questionId: "goal", prompt: "需要交付什么？", responseKind: "text", required: true, options: [] }],
    answers: null, status: "open", version: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    resolvedAtUnixMs: null, ...overrides,
  };
}

function followUp() {
  return {
    surveyId: "survey-1", surveyVersion: 1,
    followUp: { status: "processing" as const, markdown: null },
    plan: null, planFollowUp: null,
  };
}

function plan(overrides: Record<string, unknown> = {}) {
  return {
    planId: "requirement-plan:survey-1", revision: 1, projectId: "project-1",
    sourceConversationId: "conversation-1", sourceSurveyId: "survey-1", sourceRunId: "run-1",
    title: "方案", markdown: "# 方案", createdAtUnixMs: 30, confirmedAtUnixMs: null,
    ...overrides,
  };
}
