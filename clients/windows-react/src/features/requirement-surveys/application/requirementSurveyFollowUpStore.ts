import { create } from "zustand";
import type {
  RequirementSurvey,
  RequirementSurveyFollowUpSnapshot,
} from "../domain/requirementSurvey";
import type { RequirementSurveyPort } from "../ports/RequirementSurveyPort";

interface RequirementSurveyFollowUpState {
  ownerUserId: string | null;
  projectId: string | null;
  surveyId: string | null;
  surveyVersion: number;
  snapshot: RequirementSurveyFollowUpSnapshot | null;
  isLoading: boolean;
  action: "confirm" | "adopt" | null;
  error: string | null;
  sequence: number;
  requestId: number;
  activate(port: RequirementSurveyPort, ownerUserId: string, survey: RequirementSurvey): Promise<void>;
  refresh(port: RequirementSurveyPort, retainedError?: string | null): Promise<void>;
  confirm(port: RequirementSurveyPort): Promise<boolean>;
  adopt(port: RequirementSurveyPort): Promise<boolean>;
  reset(): void;
}

export const useRequirementSurveyFollowUpStore = create<RequirementSurveyFollowUpState>((set, get) => ({
  ownerUserId: null,
  projectId: null,
  surveyId: null,
  surveyVersion: 0,
  snapshot: null,
  isLoading: false,
  action: null,
  error: null,
  sequence: 0,
  requestId: 0,

  async activate(port, ownerInput, survey) {
    const ownerUserId = ownerInput.trim();
    if (!ownerUserId || survey.status !== "resolved") {
      get().reset();
      return;
    }
    const current = get();
    if (current.ownerUserId !== ownerUserId || current.projectId !== survey.projectId
      || current.surveyId !== survey.surveyId || current.surveyVersion !== survey.version) {
      set({
        ownerUserId,
        projectId: survey.projectId,
        surveyId: survey.surveyId,
        surveyVersion: survey.version,
        snapshot: null,
        isLoading: false,
        action: null,
        error: null,
        sequence: current.sequence + 1,
        requestId: 0,
      });
    }
    await get().refresh(port);
  },

  async refresh(port, retainedError = null) {
    const current = get();
    if (!current.ownerUserId || !current.projectId || !current.surveyId
      || current.requestId !== 0 || current.action) return;
    const identity = captureIdentity(current);
    const requestId = current.sequence + 1;
    set({ sequence: requestId, requestId, isLoading: current.snapshot == null, error: retainedError });
    try {
      const snapshot = await port.loadFollowUp(
        identity.ownerUserId,
        identity.projectId,
        identity.surveyId,
      );
      const state = get();
      if (!accepts(state, identity, requestId)) return;
      if (snapshot.surveyId !== identity.surveyId || snapshot.surveyVersion !== identity.surveyVersion
        || (snapshot.plan && snapshot.plan.projectId !== identity.projectId)) {
        throw new Error("需求调研后续结果身份不一致，请刷新后重试。");
      }
      set({ snapshot, isLoading: false, requestId: 0, error: retainedError });
    } catch (failure) {
      const state = get();
      if (!accepts(state, identity, requestId)) return;
      set({ isLoading: false, requestId: 0, error: retainedError ?? errorMessage(failure) });
    }
  },

  async confirm(port) {
    const current = get();
    const plan = current.snapshot?.plan;
    const planStatus = current.snapshot?.planFollowUp?.status;
    if (!plan || plan.confirmedAtUnixMs != null || current.action || current.requestId !== 0
      || (planStatus !== "completed" && planStatus !== "completed_without_reply")) return false;
    const identity = captureIdentity(current);
    const sequence = current.sequence;
    set({ action: "confirm", error: null });
    try {
      const confirmed = await port.confirmPlan(
        identity.ownerUserId,
        identity.projectId,
        identity.surveyId,
        plan.revision,
      );
      const state = get();
      if (!acceptsAction(state, identity, sequence, "confirm")) return false;
      if (confirmed.planId !== plan.planId || confirmed.revision !== plan.revision
        || confirmed.sourceSurveyId !== identity.surveyId || confirmed.projectId !== identity.projectId
        || confirmed.markdown !== plan.markdown || confirmed.confirmedAtUnixMs == null) {
        throw new Error("需求方案确认结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        snapshot: latest.snapshot ? { ...latest.snapshot, plan: confirmed } : null,
        action: null,
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (!acceptsAction(state, identity, sequence, "confirm")) return false;
      const message = errorMessage(failure);
      set({ action: null, error: message });
      await get().refresh(port, message);
      return false;
    }
  },

  async adopt(port) {
    const current = get();
    if (current.snapshot?.plan || current.snapshot?.followUp.status !== "completed"
      || !current.snapshot.followUp.markdown || current.action || current.requestId !== 0) return false;
    const identity = captureIdentity(current);
    const sequence = current.sequence;
    set({ action: "adopt", error: null });
    try {
      const confirmed = await port.adoptPlan(
        identity.ownerUserId,
        identity.projectId,
        identity.surveyId,
      );
      const state = get();
      if (!acceptsAction(state, identity, sequence, "adopt")) return false;
      if (confirmed.sourceSurveyId !== identity.surveyId || confirmed.projectId !== identity.projectId
        || confirmed.revision !== 1 || confirmed.confirmedAtUnixMs == null
        || confirmed.markdown !== current.snapshot.followUp.markdown) {
        throw new Error("历史回复保存结果不一致，请刷新后重试。");
      }
      set((latest) => ({
        snapshot: latest.snapshot ? {
          ...latest.snapshot,
          plan: confirmed,
          planFollowUp: latest.snapshot.followUp,
        } : null,
        action: null,
      }));
      return true;
    } catch (failure) {
      const state = get();
      if (!acceptsAction(state, identity, sequence, "adopt")) return false;
      const message = errorMessage(failure);
      set({ action: null, error: message });
      await get().refresh(port, message);
      return false;
    }
  },

  reset() {
    set((state) => ({
      ownerUserId: null,
      projectId: null,
      surveyId: null,
      surveyVersion: 0,
      snapshot: null,
      isLoading: false,
      action: null,
      error: null,
      sequence: state.sequence + 1,
      requestId: 0,
    }));
  },
}));

interface FollowUpIdentity {
  ownerUserId: string;
  projectId: string;
  surveyId: string;
  surveyVersion: number;
}

function captureIdentity(state: RequirementSurveyFollowUpState): FollowUpIdentity {
  if (!state.ownerUserId || !state.projectId || !state.surveyId || state.surveyVersion <= 0) {
    throw new Error("需求调研后续状态尚未激活。");
  }
  return {
    ownerUserId: state.ownerUserId,
    projectId: state.projectId,
    surveyId: state.surveyId,
    surveyVersion: state.surveyVersion,
  };
}

function accepts(
  state: RequirementSurveyFollowUpState,
  identity: FollowUpIdentity,
  requestId: number,
) {
  return matches(state, identity) && state.requestId === requestId;
}

function acceptsAction(
  state: RequirementSurveyFollowUpState,
  identity: FollowUpIdentity,
  sequence: number,
  action: "confirm" | "adopt",
) {
  return matches(state, identity) && state.sequence === sequence && state.action === action;
}

function matches(state: RequirementSurveyFollowUpState, identity: FollowUpIdentity) {
  return state.ownerUserId === identity.ownerUserId
    && state.projectId === identity.projectId
    && state.surveyId === identity.surveyId
    && state.surveyVersion === identity.surveyVersion;
}

function errorMessage(error: unknown) {
  if (error instanceof Error && error.message.trim()) return error.message;
  if (typeof error === "string" && error.trim()) return error;
  return "需求调研后续结果暂时不可用。";
}
