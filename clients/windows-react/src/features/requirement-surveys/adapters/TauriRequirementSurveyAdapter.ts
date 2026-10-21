import { invoke } from "@tauri-apps/api/core";
import {
  requirementSurveyListSchema,
  requirementSurveySchema,
  requirementSurveyFollowUpSnapshotSchema,
  requirementSurveyPlanSchema,
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

  async loadFollowUp(ownerUserId: string, projectId: string, surveyId: string) {
    return requirementSurveyFollowUpSnapshotSchema.parse(
      await invoke("load_requirement_survey_follow_up", { ownerUserId, projectId, surveyId }),
    );
  }

  async confirmPlan(
    ownerUserId: string,
    projectId: string,
    surveyId: string,
    expectedRevision: number,
  ) {
    return requirementSurveyPlanSchema.parse(
      await invoke("confirm_requirement_survey_plan", {
        input: { ownerUserId, projectId, surveyId, expectedRevision },
      }),
    );
  }

  async adoptPlan(ownerUserId: string, projectId: string, surveyId: string) {
    return requirementSurveyPlanSchema.parse(
      await invoke("adopt_requirement_survey_plan", {
        input: { ownerUserId, projectId, surveyId, expectedRevision: 0 },
      }),
    );
  }
}
