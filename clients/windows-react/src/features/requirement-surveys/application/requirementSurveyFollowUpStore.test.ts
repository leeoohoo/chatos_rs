import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  RequirementSurvey,
  RequirementSurveyFollowUpSnapshot,
  RequirementSurveyPlan,
} from "../domain/requirementSurvey";
import type { RequirementSurveyPort } from "../ports/RequirementSurveyPort";
import { useRequirementSurveyFollowUpStore } from "./requirementSurveyFollowUpStore";

describe("requirement survey follow-up store", () => {
  beforeEach(() => useRequirementSurveyFollowUpStore.getState().reset());

  it("loads the exact resolved survey and confirms only a finished current plan", async () => {
    const currentPlan = plan();
    const port = stubPort({ loadFollowUp: vi.fn(async () => snapshot({
      plan: currentPlan,
      planFollowUp: { status: "completed_without_reply", markdown: null },
    })) });
    await useRequirementSurveyFollowUpStore.getState().activate(port, "owner-1", survey());
    expect(await useRequirementSurveyFollowUpStore.getState().confirm(port)).toBe(true);
    expect(port.confirmPlan).toHaveBeenCalledWith("owner-1", "project-1", "survey-1", 1);
    expect(useRequirementSurveyFollowUpStore.getState().snapshot?.plan?.confirmedAtUnixMs).toBe(50);
  });

  it("adopts only the exact completed source reply", async () => {
    const port = stubPort({ loadFollowUp: vi.fn(async () => snapshot({
      followUp: { status: "completed", markdown: "# 历史方案" },
    })) });
    await useRequirementSurveyFollowUpStore.getState().activate(port, "owner-1", survey());
    expect(await useRequirementSurveyFollowUpStore.getState().adopt(port)).toBe(true);
    expect(port.adoptPlan).toHaveBeenCalledWith("owner-1", "project-1", "survey-1");
    expect(useRequirementSurveyFollowUpStore.getState().snapshot?.plan?.markdown).toBe("# 历史方案");
  });

  it("rejects a late follow-up after page teardown", async () => {
    let release!: (value: RequirementSurveyFollowUpSnapshot) => void;
    const port = stubPort({
      loadFollowUp: vi.fn(async () => new Promise<RequirementSurveyFollowUpSnapshot>((resolve) => { release = resolve; })),
    });
    const pending = useRequirementSurveyFollowUpStore.getState().activate(port, "owner-1", survey());
    useRequirementSurveyFollowUpStore.getState().reset();
    release(snapshot());
    await pending;
    expect(useRequirementSurveyFollowUpStore.getState().snapshot).toBeNull();
  });

  it("refreshes current Host truth when confirmation conflicts", async () => {
    const load = vi.fn()
      .mockResolvedValueOnce(snapshot({ plan: plan(), planFollowUp: { status: "completed_without_reply", markdown: null } }))
      .mockResolvedValueOnce(snapshot({ plan: plan({ revision: 2 }), planFollowUp: { status: "completed_without_reply", markdown: null } }));
    const port = stubPort({
      loadFollowUp: load,
      confirmPlan: vi.fn(async () => { throw new Error("需求方案已经变化"); }),
    });
    await useRequirementSurveyFollowUpStore.getState().activate(port, "owner-1", survey());
    expect(await useRequirementSurveyFollowUpStore.getState().confirm(port)).toBe(false);
    expect(load).toHaveBeenCalledTimes(2);
    expect(useRequirementSurveyFollowUpStore.getState().snapshot?.plan?.revision).toBe(2);
    expect(useRequirementSurveyFollowUpStore.getState().error).toContain("已经变化");
  });
});

function stubPort(overrides: Partial<RequirementSurveyPort> = {}): RequirementSurveyPort {
  const defaults: RequirementSurveyPort = {
    listProjectRequirementSurveys: vi.fn(async () => []),
    resolveProjectRequirementSurvey: vi.fn(),
    loadFollowUp: vi.fn(async () => snapshot()),
    confirmPlan: vi.fn(async () => plan({ confirmedAtUnixMs: 50 })),
    adoptPlan: vi.fn(async () => plan({ markdown: "# 历史方案", confirmedAtUnixMs: 50 })),
  };
  return { ...defaults, ...overrides } as RequirementSurveyPort;
}

function survey(overrides: Partial<RequirementSurvey> = {}): RequirementSurvey {
  return {
    surveyId: "survey-1", projectId: "project-1", sourceConversationId: "conversation-1",
    sourceRunId: "run-1", sourceTaskId: null, title: "项目目标", description: null,
    questions: [{ questionId: "goal", prompt: "目标？", responseKind: "text", required: true, options: [] }],
    answers: { goal: "Alpha" }, status: "resolved", version: 2,
    createdAtUnixMs: 10, updatedAtUnixMs: 30, resolvedAtUnixMs: 30, ...overrides,
  };
}

function snapshot(overrides: Partial<RequirementSurveyFollowUpSnapshot> = {}): RequirementSurveyFollowUpSnapshot {
  return {
    surveyId: "survey-1", surveyVersion: 2,
    followUp: { status: "processing", markdown: null },
    plan: null, planFollowUp: null, ...overrides,
  };
}

function plan(overrides: Partial<RequirementSurveyPlan> = {}): RequirementSurveyPlan {
  return {
    planId: "requirement-plan:survey-1", revision: 1, projectId: "project-1",
    sourceConversationId: "conversation-1", sourceSurveyId: "survey-1", sourceRunId: "run-1",
    title: "方案", markdown: "# 方案", createdAtUnixMs: 40, confirmedAtUnixMs: null,
    ...overrides,
  };
}
