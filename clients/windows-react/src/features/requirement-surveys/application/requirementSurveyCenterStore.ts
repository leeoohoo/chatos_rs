import { create } from "zustand";
import {
  compareRequirementSurveys,
  type RequirementSurvey,
  type RequirementSurveyAnswers,
} from "../domain/requirementSurvey";
import type { RequirementSurveyPort } from "../ports/RequirementSurveyPort";

export interface RequirementSurveyProject {
  id: string;
  title: string;
}

interface RequirementSurveyCenterState {
  ownerUserId: string | null;
  projects: RequirementSurveyProject[];
  surveys: RequirementSurvey[];
  isLoading: boolean;
  submittingSurveyIds: string[];
  error: string | null;
  sequence: number;
  requestId: number;
  activate(
    port: RequirementSurveyPort,
    ownerUserId: string,
    projects: RequirementSurveyProject[],
  ): Promise<void>;
  refresh(port: RequirementSurveyPort, retainedError?: string | null): Promise<void>;
  submit(
    port: RequirementSurveyPort,
    survey: RequirementSurvey,
    answers: RequirementSurveyAnswers,
  ): Promise<boolean>;
  reset(): void;
}

export const useRequirementSurveyCenterStore = create<RequirementSurveyCenterState>((set, get) => ({
  ownerUserId: null,
  projects: [],
  surveys: [],
  isLoading: false,
  submittingSurveyIds: [],
  error: null,
  sequence: 0,
  requestId: 0,

  async activate(port, ownerInput, projectInput) {
    const ownerUserId = ownerInput.trim();
    const projects = normalizeProjects(projectInput);
    if (!ownerUserId) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId || projectIdentity(current.projects) !== projectIdentity(projects)) {
      set({
        ownerUserId,
        projects,
        surveys: [],
        isLoading: false,
        submittingSurveyIds: [],
        error: null,
        sequence: current.sequence + 1,
        requestId: 0,
      });
    }
    if (projects.length === 0) return;
    await get().refresh(port);
  },

  async refresh(port, retainedError = null) {
    const current = get();
    if (!current.ownerUserId || current.projects.length === 0 || current.requestId !== 0
      || current.submittingSurveyIds.length > 0) return;
    const ownerUserId = current.ownerUserId;
    const projects = current.projects;
    const requestId = current.sequence + 1;
    set({
      sequence: requestId,
      requestId,
      isLoading: current.surveys.length === 0,
      error: retainedError,
    });
    const results = await Promise.allSettled(projects.map(async (project) => {
      const surveys = await port.listProjectRequirementSurveys(ownerUserId, project.id);
      if (surveys.some((survey) => survey.projectId !== project.id)) {
        throw new Error(`项目“${project.title}”返回了身份不一致的需求调研。`);
      }
      return surveys;
    }));
    const state = get();
    if (!accepts(state, ownerUserId, projects, requestId)) return;
    const surveys = results.flatMap((result, index) => result.status === "fulfilled"
      ? result.value
      : current.surveys.filter((survey) => survey.projectId === projects[index]?.id));
    const surveyIds = new Set<string>();
    const duplicate = surveys.find((survey) => {
      if (surveyIds.has(survey.surveyId)) return true;
      surveyIds.add(survey.surveyId);
      return false;
    });
    if (duplicate) {
      set({
        isLoading: false,
        requestId: 0,
        error: "需求调研身份重复，请刷新后重试。",
      });
      return;
    }
    const failures = results.filter((result): result is PromiseRejectedResult => result.status === "rejected");
    set({
      surveys: surveys.sort(compareRequirementSurveys),
      isLoading: false,
      requestId: 0,
      error: retainedError ?? aggregateLoadError(failures, projects.length),
    });
  },

  async submit(port, survey, answers) {
    const current = get();
    if (!current.ownerUserId || survey.status !== "open"
      || !current.projects.some((project) => project.id === survey.projectId)
      || current.submittingSurveyIds.includes(survey.surveyId)
      || !current.surveys.some((candidate) => (
        candidate.surveyId === survey.surveyId && candidate.version === survey.version
      ))) return false;
    const ownerUserId = current.ownerUserId;
    const projects = current.projects;
    const sequence = current.sequence;
    set({
      submittingSurveyIds: [...current.submittingSurveyIds, survey.surveyId],
      error: null,
    });
    try {
      const resolved = await port.resolveProjectRequirementSurvey(
        ownerUserId,
        survey.projectId,
        survey.surveyId,
        survey.version,
        answers,
      );
      const state = get();
      if (!acceptsMutation(state, ownerUserId, projects, sequence, survey.surveyId)) return false;
      if (resolved.surveyId !== survey.surveyId || resolved.projectId !== survey.projectId
        || resolved.status !== "resolved" || resolved.version <= survey.version) {
        throw new Error("需求调研提交结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        surveys: latest.surveys.map((candidate) => candidate.surveyId === resolved.surveyId
          ? resolved : candidate).sort(compareRequirementSurveys),
        submittingSurveyIds: latest.submittingSurveyIds.filter((id) => id !== survey.surveyId),
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (!acceptsMutation(state, ownerUserId, projects, sequence, survey.surveyId)) return false;
      const message = errorMessage(failure);
      set((latest) => ({
        submittingSurveyIds: latest.submittingSurveyIds.filter((id) => id !== survey.surveyId),
        error: message,
      }));
      await get().refresh(port, message);
      return false;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      projects: [],
      surveys: [],
      isLoading: false,
      submittingSurveyIds: [],
      error: null,
      sequence: state.sequence + 1,
      requestId: 0,
    }));
  },
}));

function normalizeProjects(projects: RequirementSurveyProject[]): RequirementSurveyProject[] {
  const identities = new Set<string>();
  return projects.flatMap((project) => {
    const id = project.id.trim();
    const title = project.title.trim();
    if (!id || !title || identities.has(id)) return [];
    identities.add(id);
    return [{ id, title }];
  });
}

function projectIdentity(projects: RequirementSurveyProject[]) {
  return projects.map((project) => `${project.id}\u0000${project.title}`).join("\u0001");
}

function accepts(
  state: RequirementSurveyCenterState,
  ownerUserId: string,
  projects: RequirementSurveyProject[],
  requestId: number,
) {
  return state.ownerUserId === ownerUserId
    && projectIdentity(state.projects) === projectIdentity(projects)
    && state.requestId === requestId;
}

function acceptsMutation(
  state: RequirementSurveyCenterState,
  ownerUserId: string,
  projects: RequirementSurveyProject[],
  sequence: number,
  surveyId: string,
) {
  return state.ownerUserId === ownerUserId
    && projectIdentity(state.projects) === projectIdentity(projects)
    && state.sequence === sequence
    && state.submittingSurveyIds.includes(surveyId);
}

function aggregateLoadError(failures: PromiseRejectedResult[], total: number): string | null {
  if (failures.length === 0) return null;
  if (failures.length === total) return errorMessage(failures[0]?.reason);
  return `有 ${failures.length} 个项目的需求调研暂时无法读取，已显示其余项目。`;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "需求调研暂时不可用。";
}
