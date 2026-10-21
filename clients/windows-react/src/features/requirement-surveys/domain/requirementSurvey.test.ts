import { describe, expect, it } from "vitest";
import {
  isRequirementSurveySubmissionValid,
  requirementSurveyFollowUpSnapshotSchema,
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

  it("binds plans and completed markdown to the exact survey", () => {
    const valid = {
      surveyId: "survey-1", surveyVersion: 2,
      followUp: { status: "processing", markdown: null },
      plan: {
        planId: "requirement-plan:survey-1", revision: 1, projectId: "project-1",
        sourceConversationId: "conversation-1", sourceSurveyId: "survey-1", sourceRunId: "run-1",
        title: "方案", markdown: "# 方案", createdAtUnixMs: 30, confirmedAtUnixMs: null,
      },
      planFollowUp: { status: "completed_without_reply", markdown: null },
    };
    expect(requirementSurveyFollowUpSnapshotSchema.parse(valid).plan?.revision).toBe(1);
    expect(() => requirementSurveyFollowUpSnapshotSchema.parse({
      ...valid, plan: { ...valid.plan, sourceSurveyId: "survey-forged" },
    })).toThrow();
    expect(() => requirementSurveyFollowUpSnapshotSchema.parse({
      ...valid, followUp: { status: "completed", markdown: null },
    })).toThrow();
    expect(() => requirementSurveyFollowUpSnapshotSchema.parse({
      ...valid, planFollowUp: null,
    })).toThrow();
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
