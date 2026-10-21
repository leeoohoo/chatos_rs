import { invoke } from "@tauri-apps/api/core";
import {
  requirementSurveyListSchema,
  requirementSurveySchema,
  type RequirementSurveyAnswers,
} from "../domain/requirementSurvey";
import type { RequirementSurveyPort } from "../ports/RequirementSurveyPort";

export class TauriRequirementSurveyAdapter implements RequirementSurveyPort {
  async listProjectRequirementSurveys(ownerUserId: string, projectId: string) {
    return requirementSurveyListSchema.parse(
      await invoke("list_project_requirement_surveys", { ownerUserId, projectId }),
    );
  }

  async resolveProjectRequirementSurvey(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
    expectedVersion: number,
    answers: RequirementSurveyAnswers,
  ) {
    return requirementSurveySchema.parse(
      await invoke("resolve_project_requirement_survey", {
        input: { ownerUserId, projectId, surveyId, expectedVersion, answers },
      }),
    );
  }
}
