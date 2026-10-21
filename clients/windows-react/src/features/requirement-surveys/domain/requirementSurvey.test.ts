import { describe, expect, it } from "vitest";
import {
  isRequirementSurveySubmissionValid,
  requirementSurveySchema,
  toggleRequirementSurveyChoice,
  type RequirementSurvey,
} from "./requirementSurvey";

describe("requirement survey domain", () => {
  it("accepts every Host-backed response kind and required answer", () => {
    const survey = fixture({
      questions: [
        { questionId: "goal", prompt: "目标？", responseKind: "text", required: true, options: [] },
        { questionId: "mode", prompt: "模式？", responseKind: "single_choice", required: true, options: ["A", "B"] },
        { questionId: "areas", prompt: "范围？", responseKind: "multiple_choice", required: true, options: ["UI", "Host"] },
        { questionId: "ship", prompt: "发布？", responseKind: "boolean", required: true, options: [] },
      ],
    });
    expect(isRequirementSurveySubmissionValid(survey, {
      goal: "交付 Alpha",
      mode: "A",
      areas: ["UI", "Host"],
      ship: false,
    })).toBe(true);
    expect(isRequirementSurveySubmissionValid(survey, {
      goal: " ", mode: "A", areas: ["UI"], ship: true,
    })).toBe(false);
  });

  it("rejects unknown projections, inconsistent status and malformed options", () => {
    expect(() => requirementSurveySchema.parse({ ...fixture(), absolutePath: "C:\\secret" })).toThrow();
    expect(() => requirementSurveySchema.parse({ ...fixture(), status: "resolved" })).toThrow();
    expect(() => requirementSurveySchema.parse({
      ...fixture(),
      questions: [{ questionId: "goal", prompt: "目标？", responseKind: "text", required: true, options: ["forged"] }],
    })).toThrow();
  });

  it("toggles multiple choices without mutating the previous answer map", () => {
    const answers = { areas: ["UI"] };
    const added = toggleRequirementSurveyChoice(answers, "areas", "Host");
    expect(added).toEqual({ areas: ["UI", "Host"] });
    expect(answers).toEqual({ areas: ["UI"] });
    expect(toggleRequirementSurveyChoice(added, "areas", "UI")).toEqual({ areas: ["Host"] });
  });
});

function fixture(overrides: Partial<RequirementSurvey> = {}): RequirementSurvey {
  return {
    surveyId: "survey-1",
    projectId: "project-1",
    sourceConversationId: "conversation-1",
    sourceRunId: "run-1",
    sourceTaskId: null,
    title: "项目目标",
    description: "请补充目标",
    questions: [{ questionId: "goal", prompt: "目标？", responseKind: "text", required: true, options: [] }],
    answers: null,
    status: "open",
    version: 1,
    createdAtUnixMs: 10,
    updatedAtUnixMs: 20,
    resolvedAtUnixMs: null,
    ...overrides,
  };
}
