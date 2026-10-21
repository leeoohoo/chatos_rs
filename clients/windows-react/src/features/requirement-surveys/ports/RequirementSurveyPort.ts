import type { RequirementSurvey, RequirementSurveyAnswers } from "../domain/requirementSurvey";

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
}
