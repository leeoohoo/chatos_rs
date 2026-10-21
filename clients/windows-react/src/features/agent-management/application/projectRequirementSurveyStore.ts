import { create } from "zustand";
import {
  compareRequirementSurveys,
  type RequirementSurvey,
  type RequirementSurveyAnswers,
} from "../../requirement-surveys/domain/requirementSurvey";
import type { AgentManagementPort } from "../ports/AgentManagementPort";

interface ProjectRequirementSurveyState {
  ownerUserId: string | null;
  projectId: string | null;
  surveys: RequirementSurvey[];
  isLoading: boolean;
  submittingSurveyId: string | null;
  error: string | null;
  sequence: number;
  requestId: number;
  activate(port: AgentManagementPort, ownerUserId: string, projectId: string): Promise<void>;
  refresh(port: AgentManagementPort): Promise<void>;
  submit(
    port: AgentManagementPort,
    survey: RequirementSurvey,
    answers: RequirementSurveyAnswers,
  ): Promise<boolean>;
  reset(): void;
}

export const useProjectRequirementSurveyStore = create<ProjectRequirementSurveyState>((set, get) => ({
  ownerUserId: null,
  projectId: null,
  surveys: [],
  isLoading: false,
  submittingSurveyId: null,
  error: null,
  sequence: 0,
  requestId: 0,

  async activate(port, ownerInput, projectInput) {
    const ownerUserId = ownerInput.trim();
    const projectId = projectInput.trim();
    if (!ownerUserId || !projectId || !port.listProjectRequirementSurveys) {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId || current.projectId !== projectId) {
      set({
        ownerUserId,
        projectId,
        surveys: [],
        isLoading: false,
        submittingSurveyId: null,
        error: null,
        sequence: current.sequence + 1,
        requestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port) {
    const current = get();
    if (!current.ownerUserId || !current.projectId || current.requestId !== 0
      || !port.listProjectRequirementSurveys) return;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const requestId = current.sequence + 1;
    set({ sequence: requestId, requestId, isLoading: current.surveys.length === 0, error: null });
    try {
      const surveys = await port.listProjectRequirementSurveys(ownerUserId, projectId);
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.projectId !== projectId || state.requestId !== requestId) return;
      if (surveys.some((survey) => survey.projectId !== projectId)) {
        throw new Error("需求调研项目身份不一致，请刷新后重试。");
      }
      set({ surveys: [...surveys].sort(compareRequirementSurveys), isLoading: false, requestId: 0 });
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.projectId !== projectId || state.requestId !== requestId) return;
      set({ isLoading: false, error: errorMessage(failure), requestId: 0 });
    }
  },

  async submit(port, survey, answers) {
    const current = get();
    if (!current.ownerUserId || !current.projectId || current.submittingSurveyId
      || !port.resolveProjectRequirementSurvey || survey.projectId !== current.projectId
      || survey.status !== "open") return false;
    const ownerUserId = current.ownerUserId;
    const projectId = current.projectId;
    const sequence = current.sequence;
    set({ submittingSurveyId: survey.surveyId, error: null });
    try {
      const resolved = await port.resolveProjectRequirementSurvey(
        ownerUserId, projectId, survey.surveyId, survey.version, answers,
      );
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.projectId !== projectId
        || state.sequence !== sequence || state.submittingSurveyId !== survey.surveyId) return false;
      if (resolved.surveyId !== survey.surveyId || resolved.projectId !== projectId
        || resolved.status !== "resolved" || resolved.version <= survey.version) {
        throw new Error("需求调研提交结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        surveys: latest.surveys.map((candidate) => candidate.surveyId === resolved.surveyId
          ? resolved : candidate).sort(compareRequirementSurveys),
        submittingSurveyId: null,
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (state.ownerUserId !== ownerUserId || state.projectId !== projectId
        || state.sequence !== sequence || state.submittingSurveyId !== survey.surveyId) return false;
      set({ submittingSurveyId: null, error: errorMessage(failure) });
      return false;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      projectId: null,
      surveys: [],
      isLoading: false,
      submittingSurveyId: null,
      error: null,
      sequence: state.sequence + 1,
      requestId: 0,
    }));
  },
}));

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "需求调研暂时不可用。";
}
