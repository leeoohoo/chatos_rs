import type {
  RequirementSurvey,
  RequirementSurveyAnswers,
  RequirementSurveyFollowUpSnapshot,
  RequirementSurveyPlan,
} from "../domain/requirementSurvey";

export interface RequirementSurveyPort {
  listProjectRequirementSurveys(
    ownerUserId: string,
    projectId: string,
  ): Promise<RequirementSurvey[]>;
  resolveProjectRequirementSurvey(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
    expectedVersion: number,
    answers: RequirementSurveyAnswers,
  ): Promise<RequirementSurvey>;
  loadFollowUp(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
  ): Promise<RequirementSurveyFollowUpSnapshot>;
  confirmPlan(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
    expectedRevision: number,
  ): Promise<RequirementSurveyPlan>;
  adoptPlan(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
  ): Promise<RequirementSurveyPlan>;
}
