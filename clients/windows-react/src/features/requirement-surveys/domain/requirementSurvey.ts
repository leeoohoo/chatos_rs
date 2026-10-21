import { z } from "zod";

export const requirementSurveyAnswerSchema = z.union([
  z.string().max(16_000),
  z.boolean(),
  z.array(z.string().max(4_000)).max(50),
]);

export const requirementSurveySchema = z.object({
  surveyId: z.string().trim().min(1).max(512),
  projectId: z.string().trim().min(1).max(1_024),
  sourceConversationId: z.string().trim().min(1).max(512),
  sourceRunId: z.string().trim().min(1).max(512),
  sourceTaskId: z.string().trim().min(1).max(512).nullable(),
  title: z.string().trim().min(1).max(1_000),
  description: z.string().max(16_000).nullable(),
  questions: z.array(z.object({
    questionId: z.string().trim().min(1).max(512),
    prompt: z.string().trim().min(1).max(4_000),
    responseKind: z.enum(["text", "single_choice", "multiple_choice", "boolean"]),
    required: z.boolean(),
    options: z.array(z.string().trim().min(1).max(4_000)).max(50),
  }).strict()).min(1).max(50),
  answers: z.record(z.string(), requirementSurveyAnswerSchema).nullable(),
  status: z.enum(["open", "resolved"]),
  version: z.number().int().positive(),
  createdAtUnixMs: z.number().int().nonnegative(),
  updatedAtUnixMs: z.number().int().nonnegative(),
  resolvedAtUnixMs: z.number().int().nonnegative().nullable(),
}).strict().superRefine((survey, context) => {
  if (survey.updatedAtUnixMs < survey.createdAtUnixMs) {
    context.addIssue({ code: "custom", message: "需求调研更新时间不能早于创建时间。", path: ["updatedAtUnixMs"] });
  }
  if ((survey.status === "resolved") !== (survey.resolvedAtUnixMs != null)) {
    context.addIssue({ code: "custom", message: "需求调研状态与完成时间不一致。", path: ["resolvedAtUnixMs"] });
  }
  if (survey.status === "resolved" && survey.answers == null) {
    context.addIssue({ code: "custom", message: "已提交调研必须包含回答。", path: ["answers"] });
  }
  const questionIds = new Set<string>();
  survey.questions.forEach((question, index) => {
    if (questionIds.has(question.questionId)) {
      context.addIssue({ code: "custom", message: "需求调研问题不能重复。", path: ["questions", index, "questionId"] });
    }
    questionIds.add(question.questionId);
    const expectsOptions = question.responseKind === "single_choice"
      || question.responseKind === "multiple_choice";
    if (expectsOptions !== (question.options.length > 0)) {
      context.addIssue({ code: "custom", message: "需求调研选项与回答类型不一致。", path: ["questions", index, "options"] });
    }
  });
  if (survey.answers) {
    const questionsById = new Map(survey.questions.map((question) => [question.questionId, question]));
    for (const [questionId, answer] of Object.entries(survey.answers)) {
      const question = questionsById.get(questionId);
      if (!question) {
        context.addIssue({ code: "custom", message: "需求调研回答引用了未知问题。", path: ["answers", questionId] });
        continue;
      }
      const valid = question.responseKind === "text"
        ? typeof answer === "string" && answer.trim().length > 0
        : question.responseKind === "boolean"
          ? typeof answer === "boolean"
          : question.responseKind === "single_choice"
            ? typeof answer === "string" && question.options.includes(answer)
            : Array.isArray(answer) && answer.length > 0
              && answer.every((value) => question.options.includes(value));
      if (!valid) {
        context.addIssue({ code: "custom", message: "需求调研回答与问题类型不一致。", path: ["answers", questionId] });
      }
    }
    if (survey.status === "resolved") {
      survey.questions.filter((question) => question.required).forEach((question) => {
        if (!(question.questionId in survey.answers!)) {
          context.addIssue({ code: "custom", message: "已提交调研缺少必填回答。", path: ["answers", question.questionId] });
        }
      });
    }
  }
});

export const requirementSurveyListSchema = z.array(requirementSurveySchema).max(200);

export const requirementSurveyFollowUpSchema = z.object({
  status: z.enum([
    "awaiting_answers",
    "processing",
    "completed",
    "completed_without_reply",
    "waiting_for_input",
    "needs_review",
    "failed",
    "cancelled",
  ]),
  markdown: z.string().trim().min(1).max(128_000).nullable(),
}).strict().superRefine((followUp, context) => {
  if ((followUp.status === "completed") !== (followUp.markdown != null)) {
    context.addIssue({ code: "custom", message: "需求调研后续状态与正文不一致。", path: ["markdown"] });
  }
});

export const requirementSurveyPlanSchema = z.object({
  planId: z.string().trim().min(1).max(1_024),
  revision: z.number().int().positive(),
  projectId: z.string().trim().min(1).max(1_024),
  sourceConversationId: z.string().trim().min(1).max(512),
  sourceSurveyId: z.string().trim().min(1).max(512),
  sourceRunId: z.string().trim().min(1).max(512),
  title: z.string().trim().min(1).max(1_000),
  markdown: z.string().trim().min(1).max(128_000),
  createdAtUnixMs: z.number().int().nonnegative(),
  confirmedAtUnixMs: z.number().int().nonnegative().nullable(),
}).strict().superRefine((plan, context) => {
  if (plan.confirmedAtUnixMs != null && plan.confirmedAtUnixMs < plan.createdAtUnixMs) {
    context.addIssue({ code: "custom", message: "需求方案确认时间不能早于创建时间。", path: ["confirmedAtUnixMs"] });
  }
});

export const requirementSurveyFollowUpSnapshotSchema = z.object({
  surveyId: z.string().trim().min(1).max(512),
  surveyVersion: z.number().int().positive(),
  followUp: requirementSurveyFollowUpSchema,
  plan: requirementSurveyPlanSchema.nullable(),
  planFollowUp: requirementSurveyFollowUpSchema.nullable(),
}).strict().superRefine((snapshot, context) => {
  if ((snapshot.plan == null) !== (snapshot.planFollowUp == null)) {
    context.addIssue({ code: "custom", message: "需求方案与来源状态不一致。", path: ["planFollowUp"] });
  }
  if (snapshot.plan && snapshot.plan.sourceSurveyId !== snapshot.surveyId) {
    context.addIssue({ code: "custom", message: "需求方案调研身份不一致。", path: ["plan", "sourceSurveyId"] });
  }
});

export type RequirementSurveyAnswer = z.infer<typeof requirementSurveyAnswerSchema>;
export type RequirementSurveyAnswers = Record<string, RequirementSurveyAnswer>;
export type RequirementSurvey = z.infer<typeof requirementSurveySchema>;
export type RequirementSurveyFollowUp = z.infer<typeof requirementSurveyFollowUpSchema>;
export type RequirementSurveyPlan = z.infer<typeof requirementSurveyPlanSchema>;
export type RequirementSurveyFollowUpSnapshot = z.infer<typeof requirementSurveyFollowUpSnapshotSchema>;

export function isRequirementSurveySubmissionValid(
  survey: RequirementSurvey,
  answers: RequirementSurveyAnswers,
): boolean {
  return survey.status === "open" && survey.questions.every((question) => {
    const answer = answers[question.questionId];
    if (answer == null) return !question.required;
    if (question.responseKind === "text") {
      return typeof answer === "string" && answer.trim().length > 0;
    }
    if (question.responseKind === "boolean") return typeof answer === "boolean";
    if (question.responseKind === "single_choice") {
      return typeof answer === "string" && question.options.includes(answer);
    }
    return Array.isArray(answer)
      && answer.length > 0
      && answer.every((value) => question.options.includes(value));
  });
}

export function toggleRequirementSurveyChoice(
  answers: RequirementSurveyAnswers,
  questionId: string,
  option: string,
): RequirementSurveyAnswers {
  const current = answers[questionId];
  const values = Array.isArray(current) ? current : [];
  const next = values.includes(option)
    ? values.filter((value) => value !== option)
    : [...values, option];
  return { ...answers, [questionId]: next };
}

export function compareRequirementSurveys(left: RequirementSurvey, right: RequirementSurvey) {
  if (left.status !== right.status) return left.status === "open" ? -1 : 1;
  return right.updatedAtUnixMs - left.updatedAtUnixMs
    || left.surveyId.localeCompare(right.surveyId);
}
